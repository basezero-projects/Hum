import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import DevConsole from "./DevConsole";
import Overlay from "./Overlay";
import "./index.css";

// Pick the component based on which Tauri window we're rendering into.
// `main` = dev console (decorated, with event log + lyrics preview).
// `overlay` = the actual transparent always-on-top lyrics window.
function pickComponent(): () => React.ReactElement {
  try {
    const label = getCurrentWindow().label;
    if (label === "overlay") return Overlay;
  } catch {
    // Not running inside Tauri (e.g. plain `vite` dev) — default to dev console.
  }
  return DevConsole;
}

const Component = pickComponent();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Component />
  </React.StrictMode>,
);
