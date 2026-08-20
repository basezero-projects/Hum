import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import DevConsole from "./DevConsole";
import Activation from "./Activation";
import Overlay from "./Overlay";
import Setup from "./Setup";
import Settings from "./Settings";
import type { BuildInfo } from "./types";
import { resolveWindowSurface, type WindowSurface } from "./window-route";
import "./index.css";

// Pick the component based on which Tauri window we're rendering into.
// `main` = dev console (decorated, with event log + lyrics preview).
// `overlay` = the transparent always-on-top lyrics window.
// `settings` = the user-facing settings window opened from the tray.
// `activation` = the purchase, activation, and license recovery window.
// `setup` = the guided first-run placement and personalization window.
function currentWindowLabel(): string | null {
  try {
    return getCurrentWindow().label;
  } catch {
    // Plain Vite development has no Tauri window label.
    return null;
  }
}

const surfaces = {
  overlay: Overlay,
  settings: Settings,
  activation: Activation,
  setup: Setup,
  dev_console: DevConsole,
} satisfies Record<WindowSurface, () => React.ReactElement>;

async function renderWindow() {
  const surface = await resolveWindowSurface(currentWindowLabel(), () =>
    invoke<BuildInfo>("get_build_info"),
  );
  const Component = surfaces[surface];

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <Component />
    </React.StrictMode>,
  );
}

void renderWindow();
