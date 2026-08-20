import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AboutInfo,
  DiagnosticExport,
  LayoutMode,
  LicenseState,
  ListeningMode,
  OverlayMode,
  OverlayShape,
  PlatformInfo,
  Settings,
  TextAlign,
  TrustDestination,
  WindowBackdrop,
} from "./types";
import { PROMO_CARD_DESCRIPTION, PROMO_CARD_LABEL } from "./promo-policy";
import {
  displayShortcut,
  SHORTCUT_ROWS,
  triggerFromKeyboardInput,
  triggerFromMouseInput,
  type ShortcutActionId,
} from "./shortcut-config";

const ACCENT = "#d4af37";
type TrustAction = "license" | "updates" | TrustDestination | "diagnostics";
type TrustFeedback = { tone: "success" | "error"; message: string };
type ShortcutFeedback = { tone: "success" | "error"; message: string };

export default function SettingsView() {
  const [s, setS] = useState<Settings | null>(null);
  const [platformInfo, setPlatformInfo] = useState<PlatformInfo | null>(null);
  const [aboutInfo, setAboutInfo] = useState<AboutInfo | null>(null);
  const [license, setLicense] = useState<LicenseState | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [trustAction, setTrustAction] = useState<TrustAction | null>(null);
  const [trustFeedback, setTrustFeedback] = useState<TrustFeedback | null>(null);
  const [recordingShortcut, setRecordingShortcut] = useState<ShortcutActionId | null>(null);
  const [shortcutFeedback, setShortcutFeedback] = useState<ShortcutFeedback | null>(null);
  // Track in-flight pending writes so slider drags coalesce.
  const writeTimer = useRef<number | null>(null);
  const pendingPatch = useRef<Partial<Settings>>({});

  useEffect(() => {
    let active = true;
    setS(null);
    setPlatformInfo(null);
    setAboutInfo(null);
    setLicense(null);
    setLoadError(null);
    Promise.all([
      invoke<Settings>("get_settings"),
      invoke<PlatformInfo>("get_platform_info"),
      invoke<AboutInfo>("get_about_info"),
      invoke<LicenseState>("get_license_state"),
    ])
      .then(([settings, info, about, licenseState]) => {
        if (!active) return;
        setS(settings);
        setPlatformInfo(info);
        setAboutInfo(about);
        setLicense(licenseState);
      })
      .catch((error: unknown) => {
        if (active) setLoadError(String(error));
      });
    const unSettings = listen<Settings>("settings-changed", (e) => setS(e.payload));
    const unLicense = listen<LicenseState>("license-state-changed", (e) =>
      setLicense(e.payload),
    );
    return () => {
      active = false;
      unSettings.then((fn) => fn()).catch(() => {});
      unLicense.then((fn) => fn()).catch(() => {});
      if (writeTimer.current) window.clearTimeout(writeTimer.current);
    };
  }, [loadAttempt]);

  useEffect(() => {
    if (!recordingShortcut) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.code === "Escape") {
        event.preventDefault();
        setRecordingShortcut(null);
        setShortcutFeedback(null);
        return;
      }
      const trigger = triggerFromKeyboardInput(event);
      if (!trigger) return;
      event.preventDefault();
      event.stopPropagation();
      void saveShortcut(recordingShortcut, trigger);
    };
    const onMouseDown = (event: MouseEvent) => {
      const trigger = triggerFromMouseInput(event);
      if (!trigger) return;
      event.preventDefault();
      event.stopPropagation();
      void saveShortcut(recordingShortcut, trigger);
    };
    const preventRecordedMouseNavigation = (event: MouseEvent) => {
      if ((event.button === 3 || event.button === 4) && event.ctrlKey && event.altKey) {
        event.preventDefault();
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("mousedown", onMouseDown, true);
    window.addEventListener("auxclick", preventRecordedMouseNavigation, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("mousedown", onMouseDown, true);
      window.removeEventListener("auxclick", preventRecordedMouseNavigation, true);
    };
  }, [recordingShortcut]);

  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    if (!s) return;
    setS({ ...s, [key]: value });
    pendingPatch.current = { ...pendingPatch.current, [key]: value };
    if (writeTimer.current) window.clearTimeout(writeTimer.current);
    writeTimer.current = window.setTimeout(() => {
      const patch = pendingPatch.current;
      pendingPatch.current = {};
      writeTimer.current = null;
      invoke<Settings>("update_settings", { patch }).catch(console.error);
    }, 200);
  }

  function updateImmediately<K extends keyof Settings>(key: K, value: Settings[K]) {
    if (!s) return;
    setS({ ...s, [key]: value });
    if (writeTimer.current) window.clearTimeout(writeTimer.current);
    const patch = { ...pendingPatch.current, [key]: value };
    pendingPatch.current = {};
    writeTimer.current = null;
    invoke<Settings>("update_settings", { patch }).catch(console.error);
  }

  async function saveShortcut(action: ShortcutActionId, trigger: string) {
    setRecordingShortcut(null);
    setShortcutFeedback(null);
    try {
      const settings = await invoke<Settings>("set_shortcut_binding", { action, trigger });
      setS(settings);
      setShortcutFeedback({
        tone: "success",
        message: `${displayShortcut(trigger)} is ready.`,
      });
    } catch (error: unknown) {
      setShortcutFeedback({ tone: "error", message: safeActionError(error) });
    }
  }

  async function restoreShortcutDefaults() {
    setRecordingShortcut(null);
    setShortcutFeedback(null);
    try {
      const settings = await invoke<Settings>("reset_shortcuts");
      setS(settings);
      setShortcutFeedback({ tone: "success", message: "Default shortcuts restored." });
    } catch (error: unknown) {
      setShortcutFeedback({ tone: "error", message: safeActionError(error) });
    }
  }

  function reset() {
    if (!confirm("Reset all settings to defaults?")) return;
    invoke<Settings>("reset_settings").then(setS).catch(console.error);
  }

  async function runTrustAction(action: TrustAction, operation: () => Promise<string>) {
    if (trustAction) return;
    setTrustAction(action);
    setTrustFeedback(null);
    try {
      setTrustFeedback({ tone: "success", message: await operation() });
    } catch (error: unknown) {
      setTrustFeedback({ tone: "error", message: safeActionError(error) });
    } finally {
      setTrustAction(null);
    }
  }

  function openTrustDestination(destination: TrustDestination) {
    void runTrustAction(destination, async () => {
      await invoke("open_trust_destination", { destination });
      return destination === "support"
        ? "Your email app should open with a Hum support message."
        : "The Hum privacy policy opened in your browser.";
    });
  }

  if (loadError) {
    return (
      <div style={pageStyle}>
        <div style={{ maxWidth: 520 }}>
          <h1 style={{ margin: "0 0 8px", fontSize: 22, fontWeight: 600 }}>
            Settings could not load
          </h1>
          <p style={{ margin: "0 0 16px", opacity: 0.7, fontSize: 13 }}>
            Hum could not read its settings or platform details. {loadError}
          </p>
          <button
            type="button"
            onClick={() => setLoadAttempt((attempt) => attempt + 1)}
            style={{ ...inputStyle, cursor: "pointer" }}
          >
            Try again
          </button>
        </div>
      </div>
    );
  }

  if (!s || !platformInfo || !aboutInfo || !license) {
    return (
      <div style={pageStyle}>
        <div style={{ opacity: 0.6 }}>Loading settings…</div>
      </div>
    );
  }

  const modeOptions: Array<[OverlayMode, string]> = [
    ["edit", "Edit"],
    ["locked", "Locked"],
  ];
  if (platformInfo.window.click_through) {
    modeOptions.push(["ghost", "Ghost (click-through)"]);
  }
  const shortcutHelp = platformInfo.services.global_shortcuts;

  return (
    <div style={pageStyle}>
      <header style={{ marginBottom: 24 }}>
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600, letterSpacing: 0.2 }}>
          Settings
        </h1>
        <p style={{ margin: "6px 0 0", opacity: 0.55, fontSize: 13 }}>
          Changes apply live to the overlay. Saved automatically.
        </p>
      </header>

      <Section title="Mode & startup">
        <Row label="Last mode (restored on launch)">
          <Select
            value={s.last_mode}
            onChange={(v) => update("last_mode", v as OverlayMode)}
            options={modeOptions}
          />
        </Row>
        {shortcutHelp ? (
          <Hint>
            Hotkey to cycle modes: {displayShortcut(s.shortcuts.cycle_mode)} (system-wide).
          </Hint>
        ) : null}
        {platformInfo.services.autostart ? (
          <>
            <Toggle
              label="Launch Hum when I sign into my PC"
              checked={s.launch_on_startup}
              onChange={(v) => update("launch_on_startup", v)}
            />
            <Hint>
              When on, Hum starts automatically when you sign in. Off by
              default. You can also manage startup apps in your operating
              system settings.
            </Hint>
          </>
        ) : null}
      </Section>

      {shortcutHelp ? (
        <Section title="Global shortcuts">
          <Hint>
            Every shortcut keeps Ctrl + Alt so it works globally without taking over normal app
            controls. Click a binding, then press Ctrl + Alt with any supported key
            {platformInfo.platform === "windows" ? " or Mouse 4 / Mouse 5" : ""}. Escape cancels.
          </Hint>
          <div style={{ marginTop: 12, borderTop: "1px solid rgba(255,255,255,0.07)" }}>
            {SHORTCUT_ROWS.map(({ action, label, detail }) => {
              const recording = recordingShortcut === action;
              return (
                <div
                  key={action}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 18,
                    minHeight: 58,
                    borderBottom: "1px solid rgba(255,255,255,0.07)",
                  }}
                >
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 600 }}>{label}</div>
                    <div style={{ marginTop: 3, fontSize: 11, opacity: 0.48 }}>{detail}</div>
                  </div>
                  <button
                    type="button"
                    aria-pressed={recording}
                    onClick={() => {
                      setShortcutFeedback(null);
                      setRecordingShortcut(recording ? null : action);
                    }}
                    style={{
                      ...inputStyle,
                      minWidth: 190,
                      cursor: "pointer",
                      borderColor: recording ? ACCENT : inputStyle.borderColor,
                      color: recording ? "#f4d66d" : inputStyle.color,
                    }}
                  >
                    {recording ? "Press Ctrl + Alt + ..." : displayShortcut(s.shortcuts[action])}
                  </button>
                </div>
              );
            })}
          </div>
          {shortcutFeedback ? (
            <div
              role="status"
              style={{
                marginTop: 12,
                color: shortcutFeedback.tone === "error" ? "#f18c8c" : "#91d7a4",
                fontSize: 12,
              }}
            >
              {shortcutFeedback.message}
            </div>
          ) : null}
          <button
            type="button"
            onClick={() => void restoreShortcutDefaults()}
            style={{ ...inputStyle, marginTop: 12, cursor: "pointer" }}
          >
            Restore shortcut defaults
          </button>
        </Section>
      ) : null}

      <Section title="Lyrics timing">
        <Row label="Listening mode">
          <SegmentedControl
            ariaLabel="Listening mode"
            value={s.listening_mode}
            onChange={(v) =>
              updateImmediately("listening_mode", v as ListeningMode)
            }
            options={[
              ["wired", "Wired"],
              ["speakers", "Speakers"],
              ["bluetooth", "Bluetooth"],
            ]}
          />
        </Row>
        <Hint>
          Pick the output you are hearing now. Hum applies its saved delay
          immediately, so switching from headphones to speakers or Bluetooth
          does not require retiming each song.
        </Hint>
        <Slider
          label={`${listeningModeLabel(s.listening_mode)} delay`}
          suffix="ms"
          min={0}
          max={2000}
          step={25}
          value={selectedProfileDelay(s)}
          onChange={(v) => update(profileDelayKey(s.listening_mode), v)}
          help="Adjust this profile once to match the audio path. The value is reused whenever you select this listening mode."
        />
        <Slider
          label="Expert calibration"
          suffix="ms"
          min={-2000}
          max={5000}
          step={25}
          value={s.anticipate_ms}
          onChange={(v) => update("anticipate_ms", v)}
          help={`Fine-tunes every listening mode. Positive values show lyrics earlier, while negative values show them later.${shortcutHelp ? ` Temporary per-song adjustment is ${displayShortcut(s.shortcuts.timing_earlier)} or ${displayShortcut(s.shortcuts.timing_later)}.` : ""}`}
        />
        <Slider
          label="Seek-jitter tolerance"
          suffix="ms"
          min={500}
          max={10000}
          step={100}
          value={s.jitter_tolerance_ms}
          onChange={(v) => update("jitter_tolerance_ms", v)}
          help="Backward position jumps smaller than this are treated as source-counter jitter, not real seeks."
        />
      </Section>

      <Section title="Typography">
        <Row label="Font family">
          <input
            type="text"
            value={s.font_family}
            onChange={(e) => update("font_family", e.target.value)}
            style={inputStyle}
          />
        </Row>
        <Slider
          label="Current line size"
          suffix="px"
          min={14}
          max={48}
          step={1}
          value={s.font_size_px}
          onChange={(v) => update("font_size_px", v)}
        />
        <Slider
          label="Current line weight"
          min={300}
          max={900}
          step={100}
          value={s.font_weight}
          onChange={(v) => update("font_weight", v)}
        />
        <Row label="Text color (current line)">
          <ColorInput
            value={s.text_color}
            onChange={(v) => update("text_color", v)}
          />
        </Row>
        <Row label="Text color (prev / next, dim)">
          <input
            type="text"
            value={s.text_color_dim}
            onChange={(e) => update("text_color_dim", e.target.value)}
            style={inputStyle}
            placeholder="rgba(255,255,255,0.45)"
          />
        </Row>
        <Row label="Text alignment">
          <Select
            value={s.text_align}
            onChange={(v) => update("text_align", v as TextAlign)}
            options={[
              ["left", "Left"],
              ["center", "Center"],
              ["right", "Right"],
            ]}
          />
        </Row>
      </Section>

      <Section title="Background">
        <Row label="Background color">
          <ColorInput
            value={s.bg_color}
            onChange={(v) => update("bg_color", v)}
          />
        </Row>
        <Slider
          label="Background opacity"
          suffix="%"
          min={0}
          max={100}
          step={1}
          value={Math.round(s.bg_opacity)}
          onChange={(v) => update("bg_opacity", v)}
          help="0% = fully transparent. Useful for dark games / videos behind the overlay."
        />
        <Toggle
          label="Tint background from album art"
          checked={s.tint_bg_from_album_art}
          onChange={(v) => update("tint_bg_from_album_art", v)}
        />
        <Hint>
          Blends the dominant color of the current track's album art into the
          background. No effect on tracks without art. Forces a minimum 22%
          opacity so the tint is visible even when Background opacity is 0.
        </Hint>
        <Toggle
          label="Blurred album art background"
          checked={s.blur_album_art_background}
          onChange={(v) => update("blur_album_art_background", v)}
        />
        <Hint>
          Paints a heavily blurred, dimmed copy of the current track's album
          art behind the lyrics in the Apple Music "Now Playing" style. Your
          background color renders on top so the opacity slider still tints
          the result. No effect on tracks without art.
          {shortcutHelp ? (
            <> Toggle on the fly with <code>{displayShortcut(s.shortcuts.toggle_blur)}</code>.</>
          ) : null}
        </Hint>
        {platformInfo.window.supported_backdrops.length > 0 ? (
          <Row label="Window backdrop">
            <Select<WindowBackdrop>
              value={s.window_backdrop}
              onChange={(v) => update("window_backdrop", v)}
              options={platformInfo.window.supported_backdrops.map((backdrop) => [
                backdrop,
                backdropLabel(backdrop),
              ])}
            />
          </Row>
        ) : null}
        <Toggle
          label="Transparent background (lyrics only)"
          checked={s.bg_hidden}
          onChange={(v) => update("bg_hidden", v)}
        />
        <Hint>
          Hides every background layer, including blurred album art, color tint,
          background paint, and the window backdrop, so only the lyrics, art,
          and media info float over the desktop.
          {shortcutHelp ? (
            <>
              Toggle on the fly with <code>{displayShortcut(s.shortcuts.toggle_transparent)}</code>.
            </>
          ) : null}
        </Hint>
      </Section>

      <Section title="Layout">
        <Row label="Overlay shape">
          <SegmentedControl<OverlayShape>
            ariaLabel="Overlay shape"
            value={s.overlay_shape}
            onChange={(v) => update("overlay_shape", v)}
            options={[
              ["ribbon", "Horizontal ribbon"],
              ["square", "Square"],
            ]}
          />
        </Row>
        <Hint>
          Square keeps the current lyric in focus with a few nearby lines.
          Turn off Transparent background to use blurred album art. Horizontal
          ribbon stays compact.
        </Hint>
        <Row label="Ribbon lyric layout">
          <Select
            value={s.layout_mode}
            onChange={(v) => update("layout_mode", v as LayoutMode)}
            options={[
              ["three_line", "3-line scroll (prev / current / next)"],
              ["single_line", "Single-line karaoke"],
              ["full_page", "Full-page scroll"],
            ]}
          />
        </Row>
        <Hint>
          This setting only changes the Horizontal ribbon. Square always uses
          its focused lyric stack.
        </Hint>
        <Slider
          label="Line padding"
          suffix="px"
          min={0}
          max={24}
          step={1}
          value={s.line_padding_px}
          onChange={(v) => update("line_padding_px", v)}
        />
      </Section>

      <Section title="Extras">
        <Toggle
          label="Show album art (when available)"
          checked={s.show_album_art}
          onChange={(v) => update("show_album_art", v)}
        />
        <Toggle
          label="Show media info column"
          checked={s.show_media}
          onChange={(v) => update("show_media", v)}
        />
        <Hint>
          Shows the track details, progress bar, and source. They sit beside the
          horizontal ribbon and along the bottom of the square view.
          {shortcutHelp ? (
            <> Toggle on the fly with <code>{displayShortcut(s.shortcuts.toggle_media)}</code>.</>
          ) : null}
        </Hint>
        <Toggle
          label="Show translated lyrics (when available)"
          checked={s.show_translation}
          onChange={(v) => update("show_translation", v)}
        />
        {platformInfo.window.screen_sampling ? (
          <>
            <Toggle
              label="Auto-contrast text (read background, invert if needed)"
              checked={s.auto_contrast}
              onChange={(v) => update("auto_contrast", v)}
            />
            <Hint>
              Samples a strip of pixels just outside the overlay every ~2s and
              flips text to white over dark backgrounds, dark over light. Useful
              when the desktop or app behind the overlay is not predictably dark.
              Overrides the Text color settings while active.
            </Hint>
          </>
        ) : null}
        <Toggle
          label={PROMO_CARD_LABEL}
          checked={s.ad_break_promos_enabled}
          onChange={(v) => update("ad_break_promos_enabled", v)}
        />
        <Hint>{PROMO_CARD_DESCRIPTION}</Hint>
      </Section>

      <Section title="Artist info panel">
        <Toggle
          label="Show artist info panel"
          checked={s.show_artist_info_panel}
          onChange={(v) => update("show_artist_info_panel", v)}
        />
        <Hint>
          Click album art (or the dot in the top corner when art is off) to view
          artist bio, similar artists, and upcoming tour dates with ticket links.
        </Hint>
        <Row label="Cache">
          <button
            onClick={() =>
              invoke("clear_artist_info_cache")
                .then(() => alert("Artist info cache cleared."))
                .catch((e: unknown) => alert(`Failed: ${e}`))
            }
            style={dangerButtonStyle}
          >
            Clear artist info cache
          </button>
        </Row>
      </Section>

      <Section title="OBS / Streamer">
        <Toggle
          label="Expose lyrics as a browser source"
          checked={s.streamer_enabled}
          onChange={(v) => update("streamer_enabled", v)}
        />
        <Row label="Port">
          <input
            type="number"
            min={1024}
            max={65535}
            value={s.streamer_port}
            onChange={(e) => update("streamer_port", Number(e.target.value))}
            style={{ ...inputStyle, width: 100, textAlign: "right" }}
          />
        </Row>
        {s.streamer_enabled ? (
          <Row label="Browser source URL">
            <CopyableUrl value={`http://localhost:${s.streamer_port}/overlay`} />
          </Row>
        ) : null}
        <Hint>
          When on, runs a local HTTP server on the selected port. Paste the
          URL above into OBS as a Browser Source (recommended size 1100×200,
          custom CSS empty). The overlay shows on your stream with a
          transparent background, no chroma key needed. Off by default
          since it opens a TCP port on localhost.
        </Hint>
      </Section>

      <Section title="About & support">
        <div style={aboutCardStyle}>
          <div>
            <div style={{ fontSize: 18, fontWeight: 700 }}>{aboutInfo.product_name}</div>
            <div style={{ marginTop: 3, fontSize: 13, opacity: 0.65 }}>
              Version {aboutInfo.version} for {platformLabel(aboutInfo.operating_system)} {" "}
              ({aboutInfo.architecture})
            </div>
          </div>
          <div style={licenseSummaryStyle}>
            <span
              aria-hidden="true"
              style={{
                ...licenseDotStyle,
                background: license.licensed ? "#77c990" : "#d99a72",
              }}
            />
            <span>{licenseSummary(license)}</span>
          </div>
        </div>

        <div style={trustActionGridStyle}>
          <button
            type="button"
            style={secondaryButtonStyle}
            disabled={trustAction !== null}
            onClick={() =>
              void runTrustAction("license", async () => {
                await invoke("open_license_window");
                return "License details opened in a separate window.";
              })
            }
          >
            {trustAction === "license" ? "Opening license" : "Manage license"}
          </button>
          <button
            type="button"
            style={secondaryButtonStyle}
            disabled={trustAction !== null || !platformInfo.services.updater}
            onClick={() =>
              void runTrustAction("updates", async () => {
                await invoke("request_update_check");
                return "Update check started. The result will appear in Hum's tray menu.";
              })
            }
          >
            {trustAction === "updates" ? "Checking" : "Check for updates"}
          </button>
          <button
            type="button"
            style={secondaryButtonStyle}
            disabled={trustAction !== null}
            onClick={() => openTrustDestination("support")}
          >
            {trustAction === "support" ? "Opening email" : "Email support"}
          </button>
          <button
            type="button"
            style={secondaryButtonStyle}
            disabled={trustAction !== null}
            onClick={() => openTrustDestination("privacy")}
          >
            {trustAction === "privacy" ? "Opening policy" : "Privacy policy"}
          </button>
        </div>

        <div style={diagnosticsRowStyle}>
          <div>
            <strong style={{ display: "block", fontSize: 13 }}>Diagnostic snapshot</strong>
            <span style={{ display: "block", marginTop: 4, fontSize: 12, opacity: 0.58 }}>
              Saves app settings and system capabilities to Downloads. Includes limited license
              status and device details. It leaves out license keys, activation details,
              verification timestamps, song information, lyrics, file paths, and cache contents.
            </span>
          </div>
          <button
            type="button"
            style={{ ...secondaryButtonStyle, flex: "0 0 auto" }}
            disabled={trustAction !== null}
            onClick={() =>
              void runTrustAction("diagnostics", async () => {
                const exported = await invoke<DiagnosticExport>("export_diagnostics");
                return `Saved diagnostic snapshot to ${exported.path}`;
              })
            }
          >
            {trustAction === "diagnostics" ? "Exporting" : "Export diagnostics"}
          </button>
        </div>

        {trustFeedback ? (
          <div
            role={trustFeedback.tone === "error" ? "alert" : "status"}
            aria-live="polite"
            style={{
              ...trustFeedbackStyle,
              borderColor:
                trustFeedback.tone === "error"
                  ? "rgba(217, 126, 114, 0.4)"
                  : "rgba(119, 201, 144, 0.34)",
              color: trustFeedback.tone === "error" ? "#f0b0a7" : "#b5dfc1",
            }}
          >
            {trustFeedback.message}
          </div>
        ) : null}
      </Section>


      <footer
        style={{
          marginTop: 24,
          paddingTop: 16,
          borderTop: "1px solid rgba(255,255,255,0.08)",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <div style={{ opacity: 0.5, fontSize: 12, display: "grid", gap: 4 }}>
          <span>
            Application data: {" "}
            <code style={{ opacity: 0.7 }}>{platformInfo.paths.app_data_dir}</code>
          </span>
          <span>
            Settings file: {" "}
            <code style={{ opacity: 0.7 }}>{platformInfo.paths.settings_file}</code>
          </span>
        </div>
        <button onClick={reset} style={dangerButtonStyle}>
          Reset to defaults
        </button>
      </footer>
    </div>
  );
}

function safeActionError(error: unknown): string {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return "Hum could not complete that action. Try again.";
}

function platformLabel(platform: string): string {
  if (platform === "windows") return "Windows";
  if (platform === "macos") return "macOS";
  if (platform === "linux") return "Linux";
  return platform;
}

function licenseSummary(license: LicenseState): string {
  if (license.status === "development") return "Development license";
  if (license.licensed) return "License active";
  if (license.status === "device_limit") return "Device limit reached";
  if (license.status === "service_unavailable") return "License check unavailable";
  return "License needs attention";
}

function backdropLabel(backdrop: WindowBackdrop): string {
  if (backdrop === "acrylic") return "Acrylic (default)";
  if (backdrop === "mica") return "Mica";
  if (backdrop === "tabbed_mica") return "Tabbed Mica";
  return "None";
}

function listeningModeLabel(mode: ListeningMode): string {
  if (mode === "bluetooth") return "Bluetooth";
  if (mode === "speakers") return "Speakers";
  return "Wired";
}

function profileDelayKey(
  mode: ListeningMode,
): "wired_delay_ms" | "speakers_delay_ms" | "bluetooth_delay_ms" {
  if (mode === "bluetooth") return "bluetooth_delay_ms";
  if (mode === "speakers") return "speakers_delay_ms";
  return "wired_delay_ms";
}

function selectedProfileDelay(settings: Settings): number {
  return settings[profileDelayKey(settings.listening_mode)];
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section style={{ marginBottom: 22 }}>
      <h2
        style={{
          margin: "0 0 10px",
          fontSize: 11,
          fontWeight: 600,
          letterSpacing: 1.2,
          textTransform: "uppercase",
          opacity: 0.55,
        }}
      >
        {title}
      </h2>
      <div style={cardStyle}>{children}</div>
    </section>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "1fr auto",
        alignItems: "center",
        gap: 12,
        padding: "10px 14px",
        borderBottom: "1px solid rgba(255,255,255,0.05)",
      }}
    >
      <label style={{ fontSize: 14 }}>{label}</label>
      <div>{children}</div>
    </div>
  );
}

function Hint({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        padding: "8px 14px",
        fontSize: 12,
        opacity: 0.5,
        borderTop: "1px solid rgba(255,255,255,0.05)",
      }}
    >
      {children}
    </div>
  );
}

function Slider({
  label,
  suffix,
  min,
  max,
  step,
  value,
  onChange,
  help,
}: {
  label: string;
  suffix?: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (v: number) => void;
  help?: string;
}) {
  return (
    <div
      style={{
        padding: "10px 14px",
        borderBottom: "1px solid rgba(255,255,255,0.05)",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 6,
        }}
      >
        <label style={{ fontSize: 14 }}>{label}</label>
        <span
          style={{
            fontSize: 13,
            fontVariantNumeric: "tabular-nums",
            color: ACCENT,
            minWidth: 60,
            textAlign: "right",
          }}
        >
          {value}
          {suffix ? ` ${suffix}` : ""}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        style={{ width: "100%", accentColor: ACCENT }}
      />
      {help ? (
        <div style={{ marginTop: 4, fontSize: 12, opacity: 0.45 }}>{help}</div>
      ) : null}
    </div>
  );
}

function Select<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: Array<[T, string]>;
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      style={{
        ...inputStyle,
        minWidth: 200,
      }}
    >
      {options.map(([v, label]) => (
        <option key={v} value={v}>
          {label}
        </option>
      ))}
    </select>
  );
}

function SegmentedControl<T extends string>({
  ariaLabel,
  value,
  onChange,
  options,
}: {
  ariaLabel: string;
  value: T;
  onChange: (v: T) => void;
  options: Array<[T, string]>;
}) {
  const moveSelection = (
    event: React.KeyboardEvent<HTMLButtonElement>,
    direction: number,
  ) => {
    const current = options.findIndex(([option]) => option === value);
    const next = (current + direction + options.length) % options.length;
    const nextValue = options[next]?.[0];
    if (!nextValue) return;
    event.preventDefault();
    onChange(nextValue);
    const group = event.currentTarget.parentElement;
    window.requestAnimationFrame(() => {
      group
        ?.querySelector<HTMLButtonElement>(`button[data-value="${nextValue}"]`)
        ?.focus();
    });
  };

  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      style={{
        display: "inline-flex",
        gap: 3,
        padding: 3,
        borderRadius: 8,
        border: "1px solid rgba(255,255,255,0.09)",
        background: "rgba(0,0,0,0.35)",
      }}
    >
      {options.map(([option, label]) => {
        const selected = option === value;
        return (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={selected}
            tabIndex={selected ? 0 : -1}
            data-value={option}
            onClick={() => onChange(option)}
            onKeyDown={(event) => {
              if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
                moveSelection(event, -1);
              } else if (
                event.key === "ArrowRight" ||
                event.key === "ArrowDown"
              ) {
                moveSelection(event, 1);
              }
            }}
            style={{
              border: selected
                ? "1px solid rgba(212,175,55,0.55)"
                : "1px solid transparent",
              borderRadius: 6,
              padding: "6px 11px",
              background: selected
                ? "rgba(212,175,55,0.14)"
                : "transparent",
              color: selected ? "#f2d77e" : "rgba(234,234,234,0.62)",
              boxShadow: selected
                ? "0 1px 8px rgba(0,0,0,0.22)"
                : "none",
              fontSize: 12,
              fontWeight: selected ? 600 : 500,
              fontFamily: "inherit",
              cursor: "pointer",
              whiteSpace: "nowrap",
              transition:
                "background 140ms ease, border-color 140ms ease, color 140ms ease",
            }}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

function ColorInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  // Native color picker only supports #rrggbb; we keep an adjacent text input
  // so users can paste rgba() too.
  const hex = /^#[0-9a-fA-F]{6}$/.test(value) ? value : "#000000";
  return (
    <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
      <input
        type="color"
        value={hex}
        onChange={(e) => onChange(e.target.value)}
        style={{ width: 36, height: 28, border: "none", background: "transparent", cursor: "pointer" }}
      />
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        style={{ ...inputStyle, width: 130, fontVariantNumeric: "tabular-nums", fontSize: 12 }}
      />
    </div>
  );
}

function CopyableUrl({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      // No-op: clipboard may be unavailable in some webviews.
    }
  };
  return (
    <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
      <code
        style={{
          ...inputStyle,
          fontFamily: "ui-monospace, SFMono-Regular, monospace",
          fontSize: 12,
          padding: "5px 10px",
        }}
      >
        {value}
      </code>
      <button
        onClick={onCopy}
        style={{
          background: "transparent",
          color: copied ? "#7ad07a" : ACCENT,
          border: `1px solid ${copied ? "rgba(122,208,122,0.5)" : "rgba(212,175,55,0.5)"}`,
          borderRadius: 6,
          padding: "5px 12px",
          fontSize: 12,
          cursor: "pointer",
          fontFamily: "inherit",
        }}
      >
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        padding: "12px 14px",
        borderBottom: "1px solid rgba(255,255,255,0.05)",
        cursor: "pointer",
        fontSize: 14,
      }}
    >
      <span>{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        style={{ width: 18, height: 18, accentColor: ACCENT, cursor: "pointer" }}
      />
    </label>
  );
}

const pageStyle: React.CSSProperties = {
  height: "100vh",
  width: "100vw",
  overflowY: "auto",
  padding: "26px 28px",
  boxSizing: "border-box",
  background: "#0e0e10",
  color: "#eaeaea",
  fontFamily: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
};

const cardStyle: React.CSSProperties = {
  background: "rgba(255,255,255,0.03)",
  border: "1px solid rgba(255,255,255,0.06)",
  borderRadius: 8,
  overflow: "hidden",
};

const aboutCardStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 18,
  padding: "16px 18px",
  borderBottom: "1px solid rgba(255,255,255,0.06)",
  background: "linear-gradient(135deg, rgba(212,175,55,0.08), rgba(255,255,255,0.015))",
};

const licenseSummaryStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  border: "1px solid rgba(255,255,255,0.08)",
  borderRadius: 999,
  padding: "6px 10px",
  fontSize: 12,
  whiteSpace: "nowrap",
};

const licenseDotStyle: React.CSSProperties = {
  width: 7,
  height: 7,
  borderRadius: "50%",
  boxShadow: "0 0 10px currentColor",
};

const trustActionGridStyle: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
  gap: 8,
  padding: "14px",
  borderBottom: "1px solid rgba(255,255,255,0.06)",
};

const secondaryButtonStyle: React.CSSProperties = {
  background: "rgba(255,255,255,0.035)",
  color: "#eaeaea",
  border: "1px solid rgba(255,255,255,0.1)",
  borderRadius: 7,
  padding: "8px 12px",
  fontSize: 12,
  fontFamily: "inherit",
  cursor: "pointer",
};

const diagnosticsRowStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 18,
  padding: "14px",
};

const trustFeedbackStyle: React.CSSProperties = {
  margin: "0 14px 14px",
  padding: "9px 11px",
  border: "1px solid",
  borderRadius: 7,
  background: "rgba(0,0,0,0.22)",
  fontSize: 12,
  lineHeight: 1.45,
  overflowWrap: "anywhere",
};

const inputStyle: React.CSSProperties = {
  background: "rgba(0,0,0,0.4)",
  color: "#eaeaea",
  border: "1px solid rgba(255,255,255,0.1)",
  borderRadius: 6,
  padding: "6px 10px",
  fontSize: 13,
  outline: "none",
  fontFamily: "inherit",
};

const dangerButtonStyle: React.CSSProperties = {
  background: "transparent",
  color: "#e57373",
  border: "1px solid rgba(229,115,115,0.4)",
  borderRadius: 6,
  padding: "6px 14px",
  fontSize: 12,
  cursor: "pointer",
  fontFamily: "inherit",
};
