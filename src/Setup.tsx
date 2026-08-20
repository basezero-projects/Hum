import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  AudioOutputDevice,
  ListeningMode,
  OnboardingState,
  OverlayShape,
  Settings,
} from "./types";
import "./setup.css";

const STEPS = ["Place it", "Match the sound", "Make it yours", "Take control"] as const;
type AppearancePreset = "album" | "panel" | "lyrics";

export default function Setup() {
  const [step, setStep] = useState(0);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [onboarding, setOnboarding] = useState<OnboardingState | null>(null);
  const [activeOutput, setActiveOutput] = useState<AudioOutputDevice | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [finishing, setFinishing] = useState(false);
  const pendingPatch = useRef<Partial<Settings>>({});
  const writeTimer = useRef<number | null>(null);
  const writeChain = useRef(Promise.resolve());
  const detectedApplied = useRef(false);

  useEffect(() => {
    let mounted = true;
    Promise.all([
      invoke<Settings>("get_settings"),
      invoke<OnboardingState>("get_onboarding_state"),
      invoke<AudioOutputDevice | null>("get_active_audio_output"),
    ])
      .then(([savedSettings, setupState, output]) => {
        if (!mounted) return;
        setSettings(savedSettings);
        setOnboarding(setupState);
        setActiveOutput(output);
      })
      .catch((reason: unknown) => {
        if (mounted) setLoadError(safeError(reason));
      });

    const settingsListener = listen<Settings>("settings-changed", (event) => {
      if (Object.keys(pendingPatch.current).length === 0) setSettings(event.payload);
    });
    const outputListener = listen<AudioOutputDevice | null>(
      "active-audio-output-changed",
      (event) => setActiveOutput(event.payload),
    );
    const openedListener = listen("onboarding-opened", () => {
      setStep(0);
      setFinishing(false);
      setActionError(null);
    });
    return () => {
      mounted = false;
      if (writeTimer.current) window.clearTimeout(writeTimer.current);
      settingsListener.then((dispose) => dispose()).catch(() => {});
      outputListener.then((dispose) => dispose()).catch(() => {});
      openedListener.then((dispose) => dispose()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!settings || !onboarding || onboarding.completed || detectedApplied.current) return;
    const detectedMode =
      onboarding.recommended_listening_mode ?? listeningModeForOutput(activeOutput);
    if (!detectedMode) return;
    detectedApplied.current = true;
    if (detectedMode && detectedMode !== settings.listening_mode) {
      stagePatch({ listening_mode: detectedMode }, true);
    }
  }, [activeOutput, onboarding, settings]);

  function stagePatch(patch: Partial<Settings>, immediate = false) {
    setSettings((current) => (current ? { ...current, ...patch } : current));
    pendingPatch.current = { ...pendingPatch.current, ...patch };
    setActionError(null);
    if (writeTimer.current) window.clearTimeout(writeTimer.current);
    if (immediate) {
      void flushPatch();
      return;
    }
    writeTimer.current = window.setTimeout(() => void flushPatch(), 140);
  }

  function flushPatch() {
    if (writeTimer.current) window.clearTimeout(writeTimer.current);
    writeTimer.current = null;
    const patch = pendingPatch.current;
    pendingPatch.current = {};
    if (Object.keys(patch).length === 0) return writeChain.current;
    const write = writeChain.current.catch(() => {}).then(() =>
      invoke<Settings>("update_settings", { patch }).then((saved) => {
        setSettings({ ...saved, ...pendingPatch.current });
      }),
    );
    writeChain.current = write;
    void write.catch((reason: unknown) => {
      setActionError(safeError(reason));
      pendingPatch.current = { ...patch, ...pendingPatch.current };
      if (writeChain.current === write) writeChain.current = Promise.resolve();
      void invoke<Settings>("get_settings").then((saved) => {
        setSettings({ ...saved, ...pendingPatch.current });
      }).catch(() => {});
    });
    return write;
  }

  function goTo(nextStep: number) {
    setActionError(null);
    setStep(Math.max(0, Math.min(STEPS.length - 1, nextStep)));
    if (nextStep === 0) void invoke("set_overlay_mode", { mode: "edit" });
  }

  async function finish() {
    if (finishing) return;
    setFinishing(true);
    setActionError(null);
    try {
      await flushPatch();
      const state = await invoke<OnboardingState>("complete_onboarding");
      setOnboarding(state);
      setFinishing(false);
      await getCurrentWindow().hide();
    } catch (reason: unknown) {
      setActionError(safeError(reason));
      setFinishing(false);
    }
  }

  if (loadError) {
    return (
      <main className="setup-shell setup-centered">
        <img src="/hum-logo.png" alt="" className="setup-error-mark" />
        <h1>Setup could not open</h1>
        <p>{loadError}</p>
        <button type="button" className="setup-primary" onClick={() => window.location.reload()}>
          Try again
        </button>
      </main>
    );
  }

  if (!settings || !onboarding) {
    return (
      <main className="setup-shell setup-centered" aria-live="polite">
        <div className="setup-loader" aria-hidden="true" />
        <span>Getting Hum ready</span>
      </main>
    );
  }

  return (
    <main className="setup-shell">
      <aside className="setup-rail">
        <div className="setup-brand">
          <img src="/hum-logo.png" alt="" />
          <span>HUM</span>
        </div>
        <div className="setup-signal" aria-hidden="true">
          {Array.from({ length: 18 }, (_, index) => (
            <i
              key={index}
              style={{
                "--bar": index,
                height: `${8 + (index % 6) * 6}px`,
              } as React.CSSProperties}
            />
          ))}
        </div>
        <p className="setup-rail-kicker">First run</p>
        <h2>Put the words where you want them.</h2>
        <ol className="setup-progress" aria-label="Setup progress">
          {STEPS.map((label, index) => (
            <li key={label} className={index === step ? "active" : index < step ? "done" : ""}>
              <button type="button" onClick={() => goTo(index)} aria-current={index === step ? "step" : undefined}>
                <span>{index < step ? <CheckIcon /> : String(index + 1).padStart(2, "0")}</span>
                {label}
              </button>
            </li>
          ))}
        </ol>
        <p className="setup-rail-note">You can run this setup again from the Hum tray menu.</p>
      </aside>

      <section className="setup-workbench">
        <header className="setup-heading">
          <div>
            <p>Step {step + 1} of {STEPS.length}</p>
            <h1>{STEPS[step]}</h1>
          </div>
          <button type="button" className="setup-skip" onClick={finish} disabled={finishing}>
            Skip setup
          </button>
        </header>

        <div className="setup-stage" aria-live="polite">
          {step === 0 ? <PlacementStep shape={settings.overlay_shape} /> : null}
          {step === 1 ? (
            <ListeningStep
              settings={settings}
              activeOutput={activeOutput}
              onPatch={stagePatch}
            />
          ) : null}
          {step === 2 ? <AppearanceStep settings={settings} onPatch={stagePatch} /> : null}
          {step === 3 ? <ControlsStep /> : null}
        </div>

        {actionError ? <div className="setup-error" role="alert">{actionError}</div> : null}

        <footer className="setup-actions">
          <button type="button" className="setup-secondary" onClick={() => goTo(step - 1)} disabled={step === 0 || finishing}>
            Back
          </button>
          <span>{onboarding.completed ? "Reviewing your saved setup" : "Changes appear on the overlay now"}</span>
          {step < STEPS.length - 1 ? (
            <button type="button" className="setup-primary" onClick={() => goTo(step + 1)} disabled={finishing}>
              Continue <ArrowIcon />
            </button>
          ) : (
            <button type="button" className="setup-primary" onClick={finish} disabled={finishing}>
              {finishing ? "Saving" : "Finish and lock"} <LockIcon />
            </button>
          )}
        </footer>
      </section>
    </main>
  );
}

function PlacementStep({ shape }: { shape: OverlayShape }) {
  return (
    <div className="setup-step placement-step">
      <div className="setup-copy">
        <span className="setup-kicker">The real overlay is open</span>
        <h2>Drag it into place.</h2>
        <p>
          Look for the gold outline on your desktop. Drag anywhere inside it, then resize from an edge. Hum remembers the position and size.
        </p>
      </div>
      <div className={`placement-diagram ${shape}`} aria-label={`${shape} overlay placement diagram`}>
        <div className="placement-screen">
          <span className="screen-toolbar" />
          <span className="screen-window one" />
          <span className="screen-window two" />
          <div className="placement-overlay">
            <span className="placement-grip"><MoveIcon /></span>
            <div>
              <small>Now singing</small>
              <strong>Words stay with the music</strong>
            </div>
            <b>EDIT</b>
          </div>
        </div>
      </div>
      <div className="placement-tip">
        <MoveIcon />
        <div><strong>Edit mode is on</strong><span>The gold edge disappears when setup locks the overlay.</span></div>
      </div>
    </div>
  );
}

function ListeningStep({
  settings,
  activeOutput,
  onPatch,
}: {
  settings: Settings;
  activeOutput: AudioOutputDevice | null;
  onPatch: (patch: Partial<Settings>, immediate?: boolean) => void;
}) {
  const delayKey = profileDelayKey(settings.listening_mode);
  const delay = settings[delayKey] as number;
  const detectedMode = listeningModeForOutput(activeOutput);
  return (
    <div className="setup-step listening-step">
      <div className="setup-copy compact">
        <span className="setup-kicker">Keep every line on time</span>
        <h2>What are you hearing?</h2>
        <p>Hum saves a separate delay for each path. Switch profiles from the tray whenever your audio moves.</p>
      </div>
      <div className={`detected-output ${activeOutput ? "found" : ""}`}>
        <OutputIcon route={activeOutput?.route ?? "unknown"} />
        <div>
          <span>{activeOutput ? "Windows output detected" : "Waiting for a Windows output"}</span>
          <strong>{activeOutput?.display_name ?? "Choose the closest match below"}</strong>
        </div>
        {detectedMode ? <b>{listeningLabel(detectedMode)}</b> : null}
      </div>
      <div className="listening-options" role="radiogroup" aria-label="Listening mode">
        {(["wired", "speakers", "bluetooth"] as ListeningMode[]).map((mode) => (
          <button
            type="button"
            role="radio"
            aria-checked={settings.listening_mode === mode}
            className={settings.listening_mode === mode ? "selected" : ""}
            onClick={() => onPatch({ listening_mode: mode }, true)}
            key={mode}
          >
            <OutputIcon route={mode} />
            <strong>{listeningLabel(mode)}</strong>
            <span>{modeDescription(mode)}</span>
            <i>{profileDelay(settings, mode)} ms</i>
          </button>
        ))}
      </div>
      <label className="delay-control">
        <span><strong>{listeningLabel(settings.listening_mode)} delay</strong><output>{delay} ms</output></span>
        <input
          type="range"
          min={0}
          max={1000}
          step={25}
          value={delay}
          onChange={(event) => onPatch({ [delayKey]: Number(event.target.value) })}
        />
        <small>Raise this if the words appear before you hear them.</small>
      </label>
    </div>
  );
}

function AppearanceStep({ settings, onPatch }: { settings: Settings; onPatch: (patch: Partial<Settings>, immediate?: boolean) => void }) {
  const preset = appearancePreset(settings);
  const choosePreset = (next: AppearancePreset) => {
    const patch: Partial<Settings> = next === "album"
      ? { bg_hidden: false, blur_album_art_background: true, show_album_art: true, bg_opacity: 0 }
      : next === "panel"
        ? { bg_hidden: false, blur_album_art_background: false, show_album_art: true, bg_opacity: 72, window_backdrop: "acrylic" }
        : { bg_hidden: true, blur_album_art_background: false };
    onPatch(patch, true);
  };
  return (
    <div className="setup-step appearance-step">
      <div className="setup-copy compact">
        <span className="setup-kicker">Start with a look you like</span>
        <h2>Shape the room around the lyrics.</h2>
        <p>These are starting points. Every detail remains available in Settings.</p>
      </div>
      <div className="shape-switch" role="radiogroup" aria-label="Overlay shape">
        {(["ribbon", "square"] as OverlayShape[]).map((shape) => (
          <button type="button" role="radio" aria-checked={settings.overlay_shape === shape} className={settings.overlay_shape === shape ? "selected" : ""} onClick={() => onPatch({ overlay_shape: shape }, true)} key={shape}>
            <ShapeIcon shape={shape} />
            <span><strong>{shape === "ribbon" ? "Ribbon" : "Square"}</strong><small>{shape === "ribbon" ? "A slim lyric strip" : "A focused lyric room"}</small></span>
          </button>
        ))}
      </div>
      <div className="appearance-options" role="radiogroup" aria-label="Appearance preset">
        <AppearanceCard id="album" title="Album atmosphere" detail="Blurred artwork fills the background" selected={preset === "album"} onClick={choosePreset} />
        <AppearanceCard id="panel" title="Clean panel" detail="A calm dark surface with clear type" selected={preset === "panel"} onClick={choosePreset} />
        <AppearanceCard id="lyrics" title="Lyrics only" detail="Words float directly over your desktop" selected={preset === "lyrics"} onClick={choosePreset} />
      </div>
    </div>
  );
}

function AppearanceCard({ id, title, detail, selected, onClick }: { id: AppearancePreset; title: string; detail: string; selected: boolean; onClick: (id: AppearancePreset) => void }) {
  return (
    <button type="button" role="radio" aria-checked={selected} className={`appearance-card ${id} ${selected ? "selected" : ""}`} onClick={() => onClick(id)}>
      <span className="appearance-swatch"><i /><i /><i /></span>
      <strong>{title}</strong>
      <small>{detail}</small>
    </button>
  );
}

function ControlsStep() {
  return (
    <div className="setup-step controls-step">
      <div className="setup-copy compact">
        <span className="setup-kicker">The three overlay modes</span>
        <h2>Move it, lock it, or let clicks pass through.</h2>
      </div>
      <div className="mode-cards">
        <article><MoveIcon /><div><strong>Edit</strong><span>Drag and resize the overlay.</span></div></article>
        <article className="selected"><LockIcon /><div><strong>Locked</strong><span>Read and use Hum without moving it.</span></div></article>
        <article><GhostIcon /><div><strong>Ghost</strong><span>Every click passes to the app below.</span></div></article>
      </div>
      <div className="shortcut-board">
        <div><kbd>Ctrl</kbd><b>+</b><kbd>Alt</kbd><b>+</b><kbd>L</kbd><span>Cycle overlay mode</span></div>
        <div><kbd>Ctrl</kbd><b>+</b><kbd>Alt</kbd><b>+</b><kbd>Left</kbd><span>Pull lyrics earlier</span></div>
        <div><kbd>Ctrl</kbd><b>+</b><kbd>Alt</kbd><b>+</b><kbd>Right</kbd><span>Push lyrics later</span></div>
        <div><kbd>Ctrl</kbd><b>+</b><kbd>Alt</kbd><b>+</b><kbd>Up / Down</kbd><span>Change lyric view</span></div>
      </div>
      <div className="finish-note"><LockIcon /><span>Finish leaves Hum in Locked mode. All shortcuts can be changed later in Settings.</span></div>
    </div>
  );
}

function listeningModeForOutput(output: AudioOutputDevice | null): ListeningMode | null {
  if (!output) return null;
  if (output.route === "wired") return "wired";
  if (output.route === "speakers" || output.route === "hdmi") return "speakers";
  if (output.route === "bluetooth") return "bluetooth";
  return null;
}

function profileDelayKey(mode: ListeningMode): "wired_delay_ms" | "speakers_delay_ms" | "bluetooth_delay_ms" {
  if (mode === "speakers") return "speakers_delay_ms";
  if (mode === "bluetooth") return "bluetooth_delay_ms";
  return "wired_delay_ms";
}

function profileDelay(settings: Settings, mode: ListeningMode) {
  return settings[profileDelayKey(mode)] as number;
}

function appearancePreset(settings: Settings): AppearancePreset {
  if (settings.bg_hidden) return "lyrics";
  if (settings.blur_album_art_background) return "album";
  return "panel";
}

function listeningLabel(mode: ListeningMode) {
  return mode === "wired" ? "Wired" : mode === "speakers" ? "Speakers" : "Bluetooth";
}

function modeDescription(mode: ListeningMode) {
  if (mode === "wired") return "Headphones or a wired line";
  if (mode === "speakers") return "PC, monitor, or room speakers";
  return "Wireless headphones or speakers";
}

function safeError(reason: unknown) {
  const message = String(reason);
  return message && message !== "undefined" && message !== "null"
    ? message
    : "Hum could not save that setup change.";
}

function CheckIcon() { return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m4 10.5 3.5 3.5L16 6" /></svg>; }
function ArrowIcon() { return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4 10h11M11 6l4 4-4 4" /></svg>; }
function MoveIcon() { return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v18M3 12h18M8 7l4-4 4 4M8 17l4 4 4-4M7 8l-4 4 4 4M17 8l4 4-4 4" /></svg>; }
function LockIcon() { return <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v2" /></svg>; }
function GhostIcon() { return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 20V10a6 6 0 0 1 12 0v10l-3-2-3 2-3-2-3 2Z" /><circle cx="10" cy="11" r=".8" /><circle cx="14" cy="11" r=".8" /></svg>; }
function ShapeIcon({ shape }: { shape: OverlayShape }) { return <svg viewBox="0 0 40 28" aria-hidden="true">{shape === "ribbon" ? <rect x="2" y="9" width="36" height="10" rx="2" /> : <rect x="9" y="2" width="22" height="24" rx="3" />}</svg>; }
function OutputIcon({ route }: { route: AudioOutputDevice["route"] | ListeningMode }) {
  if (route === "bluetooth") return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8 7 8 10V7L8 17l8-10-4-4v18l4-4" /></svg>;
  if (route === "speakers" || route === "hdmi") return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 9h3l5-4v14l-5-4H5V9ZM16 9a4 4 0 0 1 0 6M18 6a8 8 0 0 1 0 12" /></svg>;
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 13v-2a7 7 0 0 1 14 0v2M5 13h3v7H6a2 2 0 0 1-2-2v-3a2 2 0 0 1 1-2ZM19 13h-3v7h2a2 2 0 0 0 2-2v-3a2 2 0 0 0-1-2Z" /></svg>;
}
