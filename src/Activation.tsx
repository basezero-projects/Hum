import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { LicenseState, LicenseStatus } from "./types";
import "./activation.css";

type BusyAction = "activate" | "refresh" | "deactivate" | "checkout" | "portal";
type ActionError = { action: BusyAction | "load"; message: string };

const EMPTY_STATE: LicenseState = {
  status: "unlicensed",
  licensed: false,
  display_key: null,
  device_limit: 3,
  verified_at_unix_ms: null,
  verify_after_unix_ms: null,
  grace_ends_unix_ms: null,
  days_until_action: null,
  message: "Hum needs a license before it can show lyrics.",
  recovery: "Buy Hum or enter the license key from your receipt.",
};

export default function Activation() {
  const [license, setLicense] = useState<LicenseState | null>(null);
  const [key, setKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [busy, setBusy] = useState<BusyAction | null>(null);
  const [error, setError] = useState<ActionError | null>(null);

  useEffect(() => {
    let active = true;
    invoke<LicenseState>("get_license_state")
      .then((state) => {
        if (active) setLicense(state);
      })
      .catch((reason: unknown) => {
        if (!active) return;
        setLicense(EMPTY_STATE);
        setError({ action: "load", message: safeError(reason) });
      });
    const unlisten = listen<LicenseState>("license-state-changed", (event) => {
      setLicense(event.payload);
      setError(null);
    });
    return () => {
      active = false;
      unlisten.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  const presentation = useMemo(
    () => statusPresentation(license?.status ?? "unlicensed"),
    [license?.status],
  );

  async function run(action: BusyAction, operation: () => Promise<void>) {
    if (busy) return;
    setBusy(action);
    setError(null);
    try {
      await operation();
    } catch (reason: unknown) {
      setError({ action, message: safeError(reason) });
    } finally {
      setBusy(null);
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    void run("activate", async () => {
      const state = await invoke<LicenseState>("activate_license", {
        licenseKey: key,
      });
      setLicense(state);
      if (state.licensed) setKey("");
    });
  }

  function refresh() {
    void run("refresh", async () => {
      setLicense(await invoke<LicenseState>("refresh_license"));
    });
  }

  function deactivate() {
    if (!confirm("Release this PC from your Hum license?")) return;
    void run("deactivate", async () => {
      setLicense(await invoke<LicenseState>("deactivate_license"));
    });
  }

  function openCheckout() {
    void run("checkout", () => invoke("open_license_checkout"));
  }

  function openPortal() {
    void run("portal", () => invoke("open_license_portal"));
  }

  if (!license) {
    return (
      <main className="license-shell license-loading" aria-live="polite">
        <div className="license-loader" aria-hidden="true" />
        <span>Checking this PC</span>
      </main>
    );
  }

  const showActivationForm = license.status !== "development" && !license.licensed;
  const canRetry =
    ["verification_due", "offline_grace", "verification_required", "clock_error"].includes(
      license.status,
    ) ||
    (license.display_key !== null &&
      ["service_unavailable", "revoked"].includes(license.status));
  const canManageDevices =
    license.display_key !== null || ["device_limit", "revoked"].includes(license.status);

  return (
    <main className="license-shell">
      <aside className="license-rail" aria-label="Hum purchase details">
        <div className="license-echo" aria-hidden="true">
          <i />
          <i />
          <i />
        </div>
        <div className="license-brand">
          <img src="/hum-logo.png" alt="" className="license-mark" />
          <span>HUM</span>
        </div>
        <div className="license-offer">
          <span className="license-kicker">Yours for good</span>
          <div className="license-price">
            <strong>$19</strong>
            <span>one time</span>
          </div>
          <ul>
            <li>3 personal Windows devices</li>
            <li>Every Hum 1.x update</li>
            <li>30-day full refund</li>
            <li>No account required</li>
          </ul>
        </div>
        <button
          type="button"
          className="license-buy"
          onClick={openCheckout}
          disabled={busy !== null}
        >
          {busy === "checkout" ? "Opening checkout" : "Buy Hum"}
          <ArrowIcon />
        </button>
      </aside>

      <section className="license-workbench">
        <header className="license-heading">
          <span className={`license-status-dot ${presentation.tone}`} aria-hidden="true" />
          <div>
            <p>{presentation.eyebrow}</p>
            <h1>{presentation.title}</h1>
          </div>
        </header>

        <div className={`license-state-card ${presentation.tone}`} aria-live="polite">
          <StatusIcon status={license.status} />
          <div>
            <strong>{license.message}</strong>
            <p>{license.recovery}</p>
            <LicenseDeadline license={license} />
          </div>
        </div>

        {license.display_key ? (
          <div className="license-key-chip">
            <span>License</span>
            <code>{license.display_key}</code>
          </div>
        ) : null}

        {showActivationForm ? (
          <form className="license-form" onSubmit={submit}>
            <label htmlFor="hum-license-key">License key</label>
            <p>Paste the key from your Polar receipt to activate or restore Hum.</p>
            <div className="license-input-wrap">
              <KeyIcon />
              <input
                id="hum-license-key"
                type={showKey ? "text" : "password"}
                value={key}
                onChange={(event) => setKey(event.target.value)}
                placeholder="HUM-XXXX-XXXX-XXXX"
                autoComplete="off"
                spellCheck={false}
                disabled={busy !== null}
                aria-invalid={license.status === "invalid" || error?.action === "activate"}
                autoFocus
              />
              <button
                type="button"
                className="license-reveal"
                onClick={() => setShowKey((value) => !value)}
                aria-label={showKey ? "Hide license key" : "Show license key"}
              >
                {showKey ? "Hide" : "Show"}
              </button>
            </div>
            <button
              type="submit"
              className="license-primary"
              disabled={busy !== null || key.trim().length === 0}
            >
              {busy === "activate" ? "Checking key" : "Activate Hum"}
            </button>
          </form>
        ) : null}

        {error ? (
          <div className="license-error" role="alert">
            {error.message}
          </div>
        ) : null}

        <div className="license-actions">
          {canRetry ? (
            <button type="button" onClick={refresh} disabled={busy !== null}>
              {busy === "refresh" ? "Checking" : "Try verification again"}
            </button>
          ) : null}
          {canManageDevices ? (
            <button type="button" onClick={openPortal} disabled={busy !== null}>
              {busy === "portal" ? "Opening portal" : "Manage devices"}
            </button>
          ) : null}
          {license.licensed && license.status !== "development" ? (
            <button
              type="button"
              className="danger"
              onClick={deactivate}
              disabled={busy !== null}
            >
              {busy === "deactivate" ? "Releasing" : "Release this PC"}
            </button>
          ) : null}
          {license.licensed ? (
            <button
              type="button"
              className="license-done"
              onClick={() => void getCurrentWindow().hide()}
            >
              Done
            </button>
          ) : null}
        </div>

        <footer className="license-privacy">
          <ShieldIcon />
          <span>
            Your key is protected for this Windows user. Hum never places it in
            settings, links, or diagnostics.
          </span>
        </footer>
      </section>
    </main>
  );
}

function statusPresentation(status: LicenseStatus) {
  switch (status) {
    case "development":
      return { tone: "good", eyebrow: "Development build", title: "Ready to hum" };
    case "verified":
      return { tone: "good", eyebrow: "License active", title: "This PC is ready" };
    case "verification_due":
      return { tone: "notice", eyebrow: "Online check due", title: "Still fully active" };
    case "offline_grace":
      return { tone: "notice", eyebrow: "Working offline", title: "Your license still works" };
    case "device_limit":
      return { tone: "warning", eyebrow: "Three devices active", title: "Free a device first" };
    case "clock_error":
      return { tone: "warning", eyebrow: "Windows time issue", title: "Check the system clock" };
    case "revoked":
      return { tone: "danger", eyebrow: "License inactive", title: "This key needs attention" };
    case "invalid":
      return { tone: "danger", eyebrow: "Key not recognized", title: "Check the receipt key" };
    case "verification_required":
      return { tone: "danger", eyebrow: "Online check required", title: "Reconnect to continue" };
    case "service_unavailable":
      return { tone: "warning", eyebrow: "Could not connect", title: "Try again in a moment" };
    default:
      return { tone: "neutral", eyebrow: "Welcome to Hum", title: "Activate this PC" };
  }
}

function LicenseDeadline({ license }: { license: LicenseState }) {
  const timestamp =
    license.status === "offline_grace" || license.status === "verification_required"
      ? license.grace_ends_unix_ms
      : license.verify_after_unix_ms;
  if (!timestamp || license.status === "development") return null;
  const date = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  }).format(new Date(timestamp));
  const prefix = license.status === "offline_grace" ? "Offline access through" : "Next check by";
  return (
    <span className="license-deadline">
      {prefix} {date}
      {license.days_until_action !== null ? `, ${license.days_until_action} days` : ""}
    </span>
  );
}

function safeError(reason: unknown) {
  const value = String(reason);
  if (!value || value === "undefined" || value === "null") {
    return "Hum could not complete that license action.";
  }
  return value;
}

function StatusIcon({ status }: { status: LicenseStatus }) {
  const positive = ["development", "verified"].includes(status);
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      {positive ? (
        <path d="m5 12.5 4.2 4.2L19 7" />
      ) : (
        <>
          <path d="M12 8v5" />
          <path d="M12 17.2h.01" />
          <circle cx="12" cy="12" r="9" />
        </>
      )}
    </svg>
  );
}

function KeyIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="8" cy="12" r="3.5" />
      <path d="M11.5 12H21M17 12v3M14 12v2" />
    </svg>
  );
}

function ShieldIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M12 3 5.5 5.7v5.6c0 4.2 2.7 7.5 6.5 9.7 3.8-2.2 6.5-5.5 6.5-9.7V5.7L12 3Z" />
      <path d="m9.2 12 1.8 1.8 3.8-4" />
    </svg>
  );
}

function ArrowIcon() {
  return (
    <svg viewBox="0 0 20 20" aria-hidden="true">
      <path d="M4 10h11M11 6l4 4-4 4" />
    </svg>
  );
}
