export type UpdateOrigin = "automatic" | "manual";
export type UpdateErrorStage = "check" | "download" | "install" | "restart";

/**
 * How long Hum waits between automatic update checks.
 *
 * Hum is an always-on overlay, not a session app. Somebody can leave it
 * running for weeks, so a single check at startup means they never learn an
 * update exists. Six hours is frequent enough to matter and rare enough that
 * the release server never notices.
 */
export const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export type UpdateState =
  | { phase: "idle" }
  | { phase: "checking"; origin: UpdateOrigin }
  | { phase: "current"; origin: UpdateOrigin }
  | { phase: "available"; version: string }
  /**
   * The user asked to install while music was playing, so Hum is waiting for
   * a second click before it restarts. Only ever entered when a restart would
   * actually interrupt something.
   */
  | { phase: "confirming"; version: string }
  | {
      phase: "downloading";
      version: string;
      progress: number | null;
      origin: UpdateOrigin;
    }
  | { phase: "installing"; version: string }
  | { phase: "restarting"; version: string }
  | {
      phase: "error";
      stage: UpdateErrorStage;
      version?: string;
      origin?: UpdateOrigin;
      /** Human explanation from `friendlyUpdateError`. Falls back to generic copy. */
      message?: string;
    };

/**
 * How long the "click again to restart" offer stands before Hum reverts to
 * the plain install prompt.
 *
 * Without this the tray would sit on a one-click restart indefinitely, so
 * somebody opening the menu later to check for updates would instead kill
 * their own playback.
 */
export const UPDATE_CONFIRM_TIMEOUT_MS = 10_000;

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

const QUIET: UpdatePresentation = {
  bannerText: null,
  trayText: "Check for updates",
  action: "none",
  progress: null,
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

/**
 * Whether enough time has passed to run another automatic check.
 *
 * Deliberately compares wall-clock stamps rather than counting timer ticks,
 * so a machine that slept through the interval checks as soon as it wakes.
 * That covers the sleeping-laptop case with the same mechanism as the routine
 * one, instead of needing separate wake detection.
 */
export function shouldRunPeriodicCheck(
  lastCheckAtMs: number | null,
  nowMs: number,
  intervalMs: number = UPDATE_CHECK_INTERVAL_MS,
): boolean {
  if (lastCheckAtMs === null) return true;
  // A clock that moved backwards (timezone change, NTP correction) would
  // otherwise park the next check arbitrarily far in the future.
  if (nowMs < lastCheckAtMs) return true;
  return nowMs - lastCheckAtMs >= intervalMs;
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
  return (
    (state.phase === "available" || state.phase === "confirming") && hasResource
  );
}

/**
 * Whether clicking install should ask first rather than restarting straight
 * away.
 *
 * Hum exits the moment `install()` runs, so a click during playback cuts the
 * song off. Asking costs one extra click and only when it would actually
 * interrupt something, which is why this takes the playing flag rather than
 * confirming unconditionally.
 */
export function shouldConfirmInstall(
  state: UpdateState,
  isPlaying: boolean,
): boolean {
  return state.phase === "available" && isPlaying;
}

/**
 * Whether the overlay banner offers a dismiss control for this state.
 *
 * Only the two states that persist indefinitely can be dismissed. Everything
 * else clears itself within seconds, so a close button on them would be a
 * target that vanishes as the pointer arrives.
 */
export function isBannerDismissible(state: UpdateState): boolean {
  return state.phase === "available" || state.phase === "error";
}

/**
 * Identity for a dismissal, so dismissing one update does not silence the
 * next one. Errors key on their stage, which means a download failure the
 * user waved away does not hide a later install failure.
 */
export function bannerDismissKey(state: UpdateState): string | null {
  if (state.phase === "available") return `available:${state.version}`;
  if (state.phase === "error") {
    return `error:${state.stage}:${state.version ?? ""}`;
  }
  return null;
}

/**
 * Whether the overlay should be showing a banner right now.
 *
 * Both the banner itself and the native click-through hole read this, so a
 * dismissed banner cannot leave an invisible clickable region sitting over
 * the user's screen in Ghost mode.
 */
export function bannerIsVisible(
  state: UpdateState,
  dismissedKey: string | null,
): boolean {
  if (updatePresentation(state).bannerText === null) return false;
  const key = bannerDismissKey(state);
  return key === null || key !== dismissedKey;
}

/**
 * Turn a raw updater failure into something a person can act on.
 *
 * The plugin surfaces OS-level text ("os error 13", minisign failures) that
 * means nothing to somebody who just wants their lyrics back. The raw string
 * is still worth keeping for support, so callers should log it separately
 * rather than replacing it with this.
 *
 * No URL is baked in on purpose. The download page lives behind a domain that
 * is still being settled, and About already carries the real link.
 */
export function friendlyUpdateError(error: unknown): string {
  const raw = String(
    error instanceof Error ? (error.message ?? error) : error,
  ).toLowerCase();

  const has = (...needles: string[]) => needles.some((n) => raw.includes(n));

  // Windows refused to write the new build into place: the folder is locked,
  // read-only, or another copy of Hum is still holding the executable.
  if (
    has(
      "permission denied",
      "os error 13",
      "access is denied",
      "eacces",
      "operation not permitted",
      "used by another process",
      "os error 32",
    )
  ) {
    return "Windows blocked Hum from replacing itself. Close any other copy of Hum and try again, or reinstall the latest version from the Hum download page.";
  }

  // The artifact arrived but its minisign signature did not check out. Never
  // install this, and never suggest a workaround that skips verification.
  if (has("signature", "minisign", "untrusted", "verify", "verification")) {
    return "The update downloaded but could not be verified, so Hum did not install it. Reinstall the latest version from the Hum download page.";
  }

  // Ran out of room part-way through writing the artifact.
  if (has("no space", "os error 112", "disk full")) {
    return "There was not enough free disk space to download the update. Free up some space and try again.";
  }

  // Could not reach the release server at all.
  if (
    has(
      "network",
      "timed out",
      "timeout",
      "connect",
      "dns",
      "unreachable",
      "request",
      "certificate",
      "tls",
    )
  ) {
    return "Hum could not reach the update server. Check your connection and try again.";
  }

  return "The update did not go through. Try again, or reinstall the latest version from the Hum download page.";
}

export function updatePresentation(state: UpdateState): UpdatePresentation {
  switch (state.phase) {
    case "idle":
      return QUIET;
    case "checking":
      return state.origin === "manual"
        ? {
            bannerText: "Checking for updates...",
            trayText: "Checking for updates...",
            action: "none",
            progress: null,
          }
        : QUIET;
    case "current":
      return state.origin === "manual"
        ? {
            bannerText: "Hum is up to date",
            trayText: "Hum is up to date",
            action: "none",
            progress: null,
          }
        : QUIET;
    case "available":
      // Only reachable once the bytes are on disk, so this copy is literally
      // true and the click it invites really is instant.
      return {
        bannerText: `Hum v${state.version} is ready to install`,
        trayText: `Install update v${state.version}`,
        action: "install",
        progress: null,
      };
    case "confirming":
      // Names the consequence rather than the action. Somebody who opened the
      // tray to check for updates must not lose a song to a stray click.
      return {
        bannerText: `Installing stops playback and restarts Hum. Click again to install v${state.version}.`,
        trayText: `Install v${state.version} and restart now`,
        action: "install",
        progress: null,
      };
    case "downloading":
      // An automatic download is nobody's business until it finishes. Showing
      // progress for something the user never asked for turns an always-on-top
      // overlay into a nag.
      if (state.origin === "automatic") return QUIET;
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
    case "error": {
      // A failure the user never triggered stays silent. Automatic work only
      // ever checks and downloads, both of which retry on the next interval,
      // so there is nothing for them to do about it right now.
      if (state.origin === "automatic") return QUIET;

      if (state.stage === "restart") {
        return {
          bannerText:
            state.message ??
            "Hum was updated, but could not restart. Try again.",
          trayText: "Retry restart",
          action: "retry",
          progress: null,
        };
      }
      if (state.stage === "check") {
        return {
          bannerText: state.message ?? "Could not check for updates. Try again.",
          trayText: "Retry update check",
          action: "retry",
          progress: null,
        };
      }
      return {
        bannerText:
          state.message ??
          `Could not ${state.stage} Hum v${state.version ?? ""}. Try again.`,
        trayText: `Retry update${state.version ? ` v${state.version}` : ""}`,
        action: "retry",
        progress: null,
      };
    }
  }
}

export function nativeUpdateStatus(state: UpdateState): NativeUpdateStatus {
  const quiet: NativeUpdateStatus = {
    phase: "idle",
    version: null,
    progress: null,
    retryable: false,
    stage: null,
  };

  // Everything the tray hides in `updatePresentation` is hidden here too, so
  // the menu item and the banner can never disagree about whether Hum is
  // quietly working in the background.
  if (
    (state.phase === "checking" ||
      state.phase === "current" ||
      state.phase === "downloading") &&
    state.origin === "automatic"
  ) {
    return quiet;
  }
  if (state.phase === "error" && state.origin === "automatic") return quiet;

  const version = "version" in state ? (state.version ?? null) : null;
  const progress = state.phase === "downloading" ? state.progress : null;
  // `confirming` has to stay clickable, because the second click is the one
  // that actually installs.
  const retryable =
    state.phase === "available" ||
    state.phase === "confirming" ||
    (state.phase === "error" && state.origin !== "automatic");
  const stage = state.phase === "error" ? state.stage : null;
  return { phase: state.phase, version, progress, retryable, stage };
}
