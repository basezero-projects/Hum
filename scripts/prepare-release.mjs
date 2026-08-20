import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

function fail(message) {
  throw new Error(message);
}

function parseCargoVersion(text, fileLabel) {
  const match = text.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) fail(`Could not read the Hum version from ${fileLabel}`);
  return match[1];
}

function parseLockVersion(text) {
  const match = text.match(/\[\[package\]\]\s*\nname\s*=\s*"hum"\s*\nversion\s*=\s*"([^"]+)"/m);
  if (!match) fail("Could not read the Hum version from Cargo.lock");
  return match[1];
}

export async function readVersionContract(repoRoot) {
  const [packageText, cargoText, tauriText, lockText] = await Promise.all([
    readFile(path.join(repoRoot, "package.json"), "utf8"),
    readFile(path.join(repoRoot, "src-tauri", "Cargo.toml"), "utf8"),
    readFile(path.join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"),
    readFile(path.join(repoRoot, "src-tauri", "Cargo.lock"), "utf8"),
  ]);
  const versions = {
    package: JSON.parse(packageText).version,
    cargo: parseCargoVersion(cargoText, "Cargo.toml"),
    tauri: JSON.parse(tauriText).version,
    lock: parseLockVersion(lockText),
  };
  const unique = new Set(Object.values(versions));
  if (unique.size !== 1 || !versions.package) {
    fail(`Release version mismatch: ${JSON.stringify(versions)}`);
  }
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(versions.package)) {
    fail(`Invalid release version: ${versions.package}`);
  }
  return versions.package;
}

function validateTag(tag, version) {
  if (tag && tag !== `v${version}`) {
    fail(`Release tag ${tag} does not match v${version}`);
  }
}

function requiredString(value, label) {
  if (typeof value !== "string" || !value.trim()) fail(`${label} is required`);
  return value;
}

export function createWindowsSignConfig({ signtool, azureLibrary, metadata }) {
  const cmd = requiredString(signtool, "signtool path");
  const library = requiredString(azureLibrary, "Azure signing library path");
  const metadataPath = requiredString(metadata, "Azure signing metadata path");
  return {
    bundle: {
      windows: {
        signCommand: {
          cmd,
          args: [
            "sign",
            "/v",
            "/fd",
            "SHA256",
            "/tr",
            "http://timestamp.acs.microsoft.com",
            "/td",
            "SHA256",
            "/dlib",
            library,
            "/dmdf",
            metadataPath,
            "%1",
          ],
        },
      },
    },
  };
}

export async function writeWindowsSignConfig({ signtool, azureLibrary, metadata, outputPath }) {
  const config = createWindowsSignConfig({ signtool, azureLibrary, metadata });
  await writeFile(outputPath, `${JSON.stringify(config, null, 2)}\n`);
  return config;
}

function decodeBase64Strict(value, label) {
  const compact = value.trim();
  if (!compact || !/^[A-Za-z0-9+/]+={0,2}$/.test(compact)) {
    fail(`Malformed updater ${label}`);
  }
  const decoded = Buffer.from(compact, "base64");
  const roundTrip = decoded.toString("base64").replace(/=+$/, "");
  if (roundTrip !== compact.replace(/=+$/, "")) fail(`Malformed updater ${label}`);
  return decoded;
}

function signatureKeyId(signatureText, label) {
  const decoded = decodeBase64Strict(signatureText, label).toString("utf8");
  const lines = decoded.trim().split(/\r?\n/);
  if (
    lines.length < 4 ||
    !lines[0].startsWith("untrusted comment:") ||
    !lines[2].startsWith("trusted comment:")
  ) {
    fail(`Malformed updater ${label}`);
  }
  const signature = decodeBase64Strict(lines[1], label);
  decodeBase64Strict(lines[3], label);
  if (signature.length < 10) fail(`Malformed updater ${label}`);
  return signature.subarray(2, 10).toString("hex");
}

export async function verifyUpdaterKey({ repoRoot, signaturePath }) {
  const config = JSON.parse(
    await readFile(path.join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"),
  );
  const publicKey = config?.plugins?.updater?.pubkey;
  if (typeof publicKey !== "string" || !publicKey.trim()) {
    fail("Updater public key is missing from tauri.conf.json");
  }
  const publicText = decodeBase64Strict(publicKey, "public key").toString("utf8");
  const publicLines = publicText.trim().split(/\r?\n/);
  if (publicLines.length < 2 || !publicLines[0].startsWith("untrusted comment:")) {
    fail("Malformed updater public key");
  }
  const publicBytes = decodeBase64Strict(publicLines[1], "public key");
  if (publicBytes.length < 10) fail("Malformed updater public key");
  const publicKeyId = publicBytes.subarray(2, 10).toString("hex");
  const signatureKey = signatureKeyId(await readFile(signaturePath, "utf8"), "signature");
  if (publicKeyId !== signatureKey) {
    fail("Updater private key does not match the configured public key");
  }
}

function exactArtifact(files, predicate, label) {
  const matches = files.filter(predicate);
  if (matches.length !== 1) {
    fail(`Expected exactly one ${label}, found ${matches.length}`);
  }
  return matches[0];
}

async function sha256(filePath) {
  return createHash("sha256").update(await readFile(filePath)).digest("hex");
}

export async function prepareRelease({
  repoRoot,
  artifactsDir,
  outputDir,
  repository,
  tag,
  publishedAt = new Date().toISOString(),
}) {
  const version = await readVersionContract(repoRoot);
  validateTag(tag, version);
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) {
    fail("Invalid GitHub repository name");
  }

  const files = await readdir(artifactsDir);
  const expectedBase = `Hum_${version}_x64-setup`;
  const installer = exactArtifact(files, (file) => file.endsWith(".exe"), "installer");
  if (installer !== `${expectedBase}.exe`) fail(`Unexpected installer filename: ${installer}`);
  const updater = installer;
  const signature = exactArtifact(
    files,
    (file) => file.endsWith(".exe.sig"),
    "updater signature",
  );
  if (signature !== `${updater}.sig`) fail(`Unexpected updater signature filename: ${signature}`);

  const signatureText = (await readFile(path.join(artifactsDir, signature), "utf8")).trim();
  signatureKeyId(signatureText, "signature");
  await mkdir(outputDir, { recursive: true });
  await Promise.all(
    [installer, signature].map((file) =>
      cp(path.join(artifactsDir, file), path.join(outputDir, file)),
    ),
  );

  const metadata = {
    version,
    notes: `Hum v${version}`,
    pub_date: publishedAt,
    platforms: {
      "windows-x86_64": {
        signature: signatureText,
        url: `https://github.com/${repository}/releases/download/v${version}/${updater}`,
      },
    },
  };
  const proof = {
    version,
    tag: tag || null,
    repository,
    produced_at: publishedAt,
    installer: {
      file: installer,
      sha256: await sha256(path.join(artifactsDir, installer)),
    },
    updater: {
      file: updater,
      signature_file: signature,
      sha256: await sha256(path.join(artifactsDir, updater)),
    },
  };
  await writeFile(path.join(outputDir, "latest.json"), `${JSON.stringify(metadata, null, 2)}\n`);
  await writeFile(
    path.join(outputDir, "release-proof.json"),
    `${JSON.stringify(proof, null, 2)}\n`,
  );
  return { version, metadata, proof };
}

function parseArgs(args) {
  const command = args[0];
  const values = {};
  for (let index = 1; index < args.length; index += 2) {
    const key = args[index];
    const value = args[index + 1];
    if (!key?.startsWith("--") || value === undefined) fail(`Invalid argument: ${key ?? ""}`);
    values[key.slice(2)] = value;
  }
  return { command, values };
}

async function main() {
  const { command, values } = parseArgs(process.argv.slice(2));
  const repoRoot = path.resolve(values["repo-root"] ?? ".");
  if (command === "check-version") {
    const version = await readVersionContract(repoRoot);
    validateTag(values.tag, version);
    process.stdout.write(`${version}\n`);
    return;
  }
  if (command === "verify-key") {
    if (!values.signature) fail("verify-key requires --signature");
    await verifyUpdaterKey({ repoRoot, signaturePath: path.resolve(values.signature) });
    process.stdout.write("Updater key matches the configured public key\n");
    return;
  }
  if (command === "write-sign-config") {
    for (const required of ["signtool", "azure-library", "metadata", "output"]) {
      if (!values[required]) fail(`write-sign-config requires --${required}`);
    }
    await writeWindowsSignConfig({
      signtool: values.signtool,
      azureLibrary: values["azure-library"],
      metadata: values.metadata,
      outputPath: path.resolve(values.output),
    });
    process.stdout.write("Prepared the Windows signing configuration\n");
    return;
  }
  if (command === "prepare") {
    for (const required of ["artifacts", "output", "repository"]) {
      if (!values[required]) fail(`prepare requires --${required}`);
    }
    const result = await prepareRelease({
      repoRoot,
      artifactsDir: path.resolve(values.artifacts),
      outputDir: path.resolve(values.output),
      repository: values.repository,
      tag: values.tag || undefined,
    });
    process.stdout.write(`Prepared Hum v${result.version}\n`);
    return;
  }
  fail("Expected check-version, verify-key, write-sign-config, or prepare command");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
