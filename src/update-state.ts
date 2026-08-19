export type UpdateOrigin = "automatic" | "manual";
export type UpdateErrorStage = "check" | "download" | "install" | "restart";

export type UpdateState =
  | { phase: "idle" }
  | { phase: "checking"; origin: UpdateOrigin }
  | { phase: "current"; origin: UpdateOrigin }
  | { phase: "available"; version: string }
  | { phase: "downloading"; version: string; progress: number | null }
  | { phase: "installing"; version: string }
  | { phase: "restarting"; version: string }
  | {
      phase: "error";
      stage: UpdateErrorStage;
      version?: string;
      origin?: UpdateOrigin;
    };

export type UpdateAction = "none" | "install" | "retry";
export type UpdateOperation = "idle" | "checking" | "installing";

export type UpdatePresentation = {
  bannerText: string | null;
  trayText: string;
  action: UpdateAction;
  progress: number | null;
};

export type NativeUpdateStatus = {
  phase: UpdateState["phase"];
  version: string | null;
  progress: number | null;
  retryable: boolean;
  stage: UpdateErrorStage | null;
};

export function sanitizeUpdateVersion(version: string): string | null {
  const value = version.trim();
  if (
    value.length === 0 ||
    value.length > 32 ||
    !/^[0-9A-Za-z.+-]+$/.test(value)
  ) {
    return null;
  }
  return value;
}

export function promoteUpdateOrigin(
  current: UpdateOrigin,
  requested: UpdateOrigin,
): UpdateOrigin {
  return current === "manual" || requested === "manual" ? "manual" : "automatic";
}

export function shouldRequestManualCheck(
  action: UpdateAction,
  operation: UpdateOperation,
): boolean {
  return action === "none" && operation !== "installing";
}

export function clampDownloadProgress(
  downloadedBytes: number,
  totalBytes: number | undefined,
): number | null {
  if (!Number.isFinite(totalBytes) || (totalBytes ?? 0) <= 0) return null;
  const percent = Math.round((downloadedBytes / totalBytes!) * 100);
  return Math.min(100, Math.max(0, percent));
}

export function canInstallUpdate(
  state: UpdateState,
  hasResource: boolean,
): boolean {
  return state.phase === "available" && hasResource;
}

export function updatePresentation(state: UpdateState): UpdatePresentation {
  switch (state.phase) {
    case "idle":
      return {
        bannerText: null,
        trayText: "Check for updates",
        action: "none",
        progress: null,
      };
    case "checking":
      return state.origin === "manual"
        ? {
            bannerText: "Checking for updates...",
            trayText: "Checking for updates...",
            action: "none",
            progress: null,
          }
        : {
            bannerText: null,
            trayText: "Check for updates",
            action: "none",
            progress: null,
          };
    case "current":
      return state.origin === "manual"
        ? {
            bannerText: "Hum is up to date",
            trayText: "Hum is up to date",
            action: "none",
            progress: null,
          }
        : {
            bannerText: null,
            trayText: "Check for updates",
            action: "none",
            progress: null,
          };
    case "available":
      return {
        bannerText: `Hum v${state.version} is ready to install`,
        trayText: `Install update v${state.version}`,
        action: "install",
        progress: null,
      };
    case "downloading":
      return {
        bannerText:
          state.progress === null
            ? `Downloading Hum v${state.version}...`
            : `Downloading Hum v${state.version}: ${state.progress}%`,
        trayText:
          state.progress === null
            ? "Downloading update..."
            : `Downloading update: ${state.progress}%`,
        action: "none",
        progress: state.progress,
      };
    case "installing":
      return {
        bannerText: `Installing Hum v${state.version}...`,
        trayText: "Installing update...",
        action: "none",
        progress: null,
      };
    case "restarting":
      return {
        bannerText: "Restarting Hum...",
        trayText: "Restarting Hum...",
        action: "none",
        progress: null,
      };
    case "error":
      if (state.stage === "check") {
        return state.origin === "automatic"
          ? {
              bannerText: null,
              trayText: "Check for updates",
              action: "none",
              progress: null,
            }
          : {
              bannerText: "Could not check for updates. Try again.",
              trayText: "Retry update check",
              action: "retry",
              progress: null,
            };
      }
      if (state.stage === "restart") {
        return {
          bannerText: "Hum was updated, but could not restart. Try again.",
          trayText: "Retry restart",
          action: "retry",
          progress: null,
        };
      }
      return {
        bannerText: `Could not ${state.stage} Hum v${state.version ?? ""}. Try again.`,
        trayText: `Retry update${state.version ? ` v${state.version}` : ""}`,
        action: "retry",
        progress: null,
      };
  }
}

export function nativeUpdateStatus(state: UpdateState): NativeUpdateStatus {
  if (
    (state.phase === "checking" || state.phase === "current") &&
    state.origin === "automatic"
  ) {
    return {
      phase: "idle",
      version: null,
      progress: null,
      retryable: false,
      stage: null,
    };
  }
  if (state.phase === "error" && state.stage === "check" && state.origin === "automatic") {
    return {
      phase: "idle",
      version: null,
      progress: null,
      retryable: false,
      stage: null,
    };
  }

  const version = "version" in state ? (state.version ?? null) : null;
  const progress = state.phase === "downloading" ? state.progress : null;
  const retryable =
    state.phase === "available" ||
    (state.phase === "error" && state.origin !== "automatic");
  const stage = state.phase === "error" ? state.stage : null;
  return { phase: state.phase, version, progress, retryable, stage };
}
