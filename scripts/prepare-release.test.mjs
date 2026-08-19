import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

import * as releaseContract from "./prepare-release.mjs";

const { prepareRelease } = releaseContract;
const execFileAsync = promisify(execFile);

const VERSION = "1.2.3";

function validSignature() {
  const raw = [
    "untrusted comment: signature from minisign secret key",
    Buffer.alloc(74, 7).toString("base64"),
    "trusted comment: timestamp:1750000000",
    Buffer.alloc(64, 9).toString("base64"),
    "",
  ].join("\n");
  return Buffer.from(raw, "utf8").toString("base64");
}

async function fixture(options = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "hum-release-"));
  const artifactsDir = path.join(root, "artifacts");
  const outputDir = path.join(root, "output");
  await mkdir(artifactsDir);

  const versions = options.versions ?? {
    package: VERSION,
    cargo: VERSION,
    tauri: VERSION,
    lock: VERSION,
  };
  await writeFile(path.join(root, "package.json"), JSON.stringify({ version: versions.package }));
  await mkdir(path.join(root, "src-tauri"));
  await writeFile(path.join(root, "src-tauri", "Cargo.toml"), `[package]\nname = "hum"\nversion = "${versions.cargo}"\n`);
  await writeFile(path.join(root, "src-tauri", "tauri.conf.json"), JSON.stringify({ version: versions.tauri }));
  await writeFile(path.join(root, "src-tauri", "Cargo.lock"), `[[package]]\nname = "hum"\nversion = "${versions.lock}"\n`);

  if (options.installer !== false) {
    await writeFile(path.join(artifactsDir, `Hum_${VERSION}_x64-setup.exe`), "installer");
  }
  if (options.archive !== false) {
    await writeFile(path.join(artifactsDir, `Hum_${VERSION}_x64-setup.nsis.zip`), "updater");
  }
  if (options.signature !== false) {
    await writeFile(
      path.join(artifactsDir, `Hum_${VERSION}_x64-setup.nsis.zip.sig`),
      options.signatureText ?? validSignature(),
    );
  }
  if (options.extraInstaller) {
    await writeFile(path.join(artifactsDir, "Hum_extra_x64-setup.exe"), "extra");
  }

  return { root, artifactsDir, outputDir };
}

async function runFixture(options = {}) {
  const dirs = await fixture(options);
  const result = await prepareRelease({
    repoRoot: dirs.root,
    artifactsDir: dirs.artifactsDir,
    outputDir: dirs.outputDir,
    repository: "basezero-projects/Hum",
    tag: options.tag,
    publishedAt: "2026-08-19T12:00:00.000Z",
  });
  return { ...dirs, result };
}

test("release preparation rejects version drift", async () => {
  await assert.rejects(
    runFixture({ versions: { package: VERSION, cargo: "1.2.4", tauri: VERSION, lock: VERSION } }),
    /version/i,
  );
});

test("release preparation rejects missing and extra installers", async () => {
  await assert.rejects(runFixture({ installer: false }), /installer/i);
  await assert.rejects(runFixture({ extraInstaller: true }), /installer/i);
});

test("release preparation rejects missing or malformed updater signatures", async () => {
  await assert.rejects(runFixture({ signature: false }), /signature/i);
  await assert.rejects(runFixture({ signatureText: "not-a-minisign-signature" }), /signature/i);
});

test("release preparation rejects a tag that does not match the version", async () => {
  await assert.rejects(runFixture({ tag: "v9.9.9" }), /tag/i);
});

test("valid release input creates exact Windows metadata and proof", async () => {
  const { outputDir, result } = await runFixture({ tag: `v${VERSION}` });
  assert.equal(result.version, VERSION);
  const metadata = JSON.parse(await readFile(path.join(outputDir, "latest.json"), "utf8"));
  assert.deepEqual(metadata, {
    version: VERSION,
    notes: `Hum v${VERSION}`,
    pub_date: "2026-08-19T12:00:00.000Z",
    platforms: {
      "windows-x86_64": {
        signature: validSignature(),
        url: `https://github.com/basezero-projects/Hum/releases/download/v${VERSION}/Hum_${VERSION}_x64-setup.nsis.zip`,
      },
    },
  });
  const proof = JSON.parse(await readFile(path.join(outputDir, "release-proof.json"), "utf8"));
  assert.equal(proof.version, VERSION);
  assert.equal(proof.installer.file, `Hum_${VERSION}_x64-setup.exe`);
  assert.equal(proof.updater.file, `Hum_${VERSION}_x64-setup.nsis.zip`);
  assert.match(proof.installer.sha256, /^[a-f0-9]{64}$/);
  assert.match(proof.updater.sha256, /^[a-f0-9]{64}$/);
});

test("release workflow keeps proof runs private and tag releases signed", async () => {
  const workflow = await readFile(new URL("../.github/workflows/release.yml", import.meta.url), "utf8");
  assert.match(workflow, /workflow_dispatch:/);
  assert.match(workflow, /tags:\s*\n\s*- ['"]v\*['"]/);
  assert.match(workflow, /Microsoft\.Trusted\.Signing\.Client/);
  assert.match(workflow, /prepare-release\.mjs write-sign-config/);
  assert.match(workflow, /--signtool "\$signtool" --azure-library "\$dll"/);
  assert.doesNotMatch(workflow, /Copy-Item \$signtool signtool\.exe/);
  assert.match(workflow, /TAURI_SIGNING_PRIVATE_KEY/);
  assert.doesNotMatch(workflow, /Get-AuthenticodeSignature/);
  assert.match(workflow, /"SIGNTOOL_PATH=\$signtool" \| Add-Content \$env:GITHUB_ENV/);
  assert.doesNotMatch(workflow, /\$targets = @\('src-tauri\/target\/release\/hum\.exe'/);
  assert.doesNotMatch(workflow, /Get-Command 7z/);
  assert.match(workflow, /\$sevenZip = 'C:\\Program Files\\7-Zip\\7z\.exe'/);
  assert.match(workflow, /7z\.exe was not found/);
  assert.match(workflow, /& \$sevenZip x -y "-o\$extractRoot" \$installer\[0\]\.FullName/);
  assert.match(workflow, /Get-ChildItem \$extractRoot -Recurse -File -Filter 'hum\.exe'/);
  assert.match(workflow, /Expected one installed Hum executable, found \$\(\$installedApp\.Count\)/);
  assert.match(workflow, /\$targets = @\(\$installedApp\[0\]\.FullName, \$installer\[0\]\.FullName\)/);
  assert.match(workflow, /& \$env:SIGNTOOL_PATH verify \/pa \/v \$target/);
  assert.match(workflow, /if \(\$LASTEXITCODE -ne 0\)/);
  assert.match(workflow, /SignTool exited \$LASTEXITCODE/);
  assert.match(workflow, /prepare-release\.mjs/);
  assert.match(
    workflow,
    /if: github\.event_name == 'push' && startsWith\(github\.ref, 'refs\/tags\/v'\)/,
  );
  assert.doesNotMatch(workflow, /if: github\.event_name == 'workflow_dispatch'\s*\n\s*uses: .*release/i);
});

test("Windows signing config preserves exact spaced paths and Tauri placeholder", async () => {
  assert.equal(typeof releaseContract.writeWindowsSignConfig, "function");
  const signtool = String.raw`C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe`;
  const azureLibrary = String.raw`D:\a\Hum build\signing\Azure.CodeSigning.Dlib.dll`;
  const metadata = String.raw`D:\a\Hum build\signing metadata.json`;
  const root = await mkdtemp(path.join(os.tmpdir(), "hum-sign-config-"));
  const outputPath = path.join(root, "sign config.json");
  await releaseContract.writeWindowsSignConfig({ signtool, azureLibrary, metadata, outputPath });
  assert.deepEqual(
    JSON.parse(await readFile(outputPath, "utf8")),
    {
      bundle: {
        windows: {
          signCommand: {
            cmd: signtool,
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
              azureLibrary,
              "/dmdf",
              metadata,
              "%1",
            ],
          },
        },
      },
    },
  );
});

test("release metadata ships only Hum and keeps the UI inspector as an example", async () => {
  const srcTauri = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../src-tauri");
  const { stdout } = await execFileAsync(
    "cargo",
    ["metadata", "--no-deps", "--format-version", "1"],
    { cwd: srcTauri },
  );
  const metadata = JSON.parse(stdout);
  const hum = metadata.packages.find((entry) => entry.name === "hum");
  assert.ok(hum);
  assert.deepEqual(
    hum.targets.filter((target) => target.kind.includes("bin")).map((target) => target.name),
    ["hum"],
  );
  assert.deepEqual(
    hum.targets.filter((target) => target.kind.includes("example")).map((target) => target.name),
    ["dump_uia"],
  );

  const inspectorSource = await readFile(
    path.join(srcTauri, "examples", "dump_uia", "windows.rs"),
    "utf8",
  );
  assert.doesNotMatch(inspectorSource, /cargo run --bin dump_uia/);
  assert.match(inspectorSource, /cargo run --example dump_uia/);
});
