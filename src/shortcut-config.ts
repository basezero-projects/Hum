export type ShortcutActionId =
  | "cycle_mode"
  | "timing_earlier"
  | "timing_later"
  | "view_previous"
  | "view_next"
  | "toggle_blur"
  | "toggle_transparent"
  | "toggle_media";

export type ShortcutBindings = Record<ShortcutActionId, string>;

export const SHORTCUT_ROWS: ReadonlyArray<{
  action: ShortcutActionId;
  label: string;
  detail: string;
}> = [
  { action: "timing_earlier", label: "Pull lyrics earlier", detail: "Temporary song timing" },
  { action: "timing_later", label: "Push lyrics later", detail: "Temporary song timing" },
  { action: "view_previous", label: "Previous lyric view", detail: "Ribbon layouts and square" },
  { action: "view_next", label: "Next lyric view", detail: "Ribbon layouts and square" },
  { action: "cycle_mode", label: "Cycle interaction mode", detail: "Edit, Locked, and Ghost" },
  { action: "toggle_blur", label: "Toggle album blur", detail: "Overlay appearance" },
  { action: "toggle_transparent", label: "Toggle transparent mode", detail: "Lyrics-only appearance" },
  { action: "toggle_media", label: "Toggle media details", detail: "Album art and track information" },
];

type KeyboardInput = {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
};

type MouseInput = {
  button: number;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
};

const MODIFIER_CODES = new Set([
  "AltLeft",
  "AltRight",
  "ControlLeft",
  "ControlRight",
  "MetaLeft",
  "MetaRight",
  "ShiftLeft",
  "ShiftRight",
]);

export function triggerFromKeyboardInput(input: KeyboardInput): string | null {
  if (!hasExactGlobalChord(input) || MODIFIER_CODES.has(input.code) || input.code === "Escape") {
    return null;
  }
  return /^[A-Za-z0-9]+$/.test(input.code) ? input.code : null;
}

export function triggerFromMouseInput(input: MouseInput): "Mouse4" | "Mouse5" | null {
  if (!hasExactGlobalChord(input)) return null;
  if (input.button === 3) return "Mouse4";
  if (input.button === 4) return "Mouse5";
  return null;
}

export function displayShortcut(trigger: string): string {
  return `Ctrl + Alt + ${displayTrigger(trigger)}`;
}

function hasExactGlobalChord(input: {
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
}): boolean {
  return input.ctrlKey && input.altKey && !input.shiftKey && !input.metaKey;
}

function displayTrigger(trigger: string): string {
  const named: Record<string, string> = {
    ArrowLeft: "Left",
    ArrowRight: "Right",
    ArrowUp: "Up",
    ArrowDown: "Down",
    Mouse4: "Mouse 4",
    Mouse5: "Mouse 5",
    Space: "Space",
    BracketLeft: "[",
    BracketRight: "]",
    Semicolon: ";",
    Quote: "'",
    Comma: ",",
    Period: ".",
    Slash: "/",
    Backslash: "\\",
    Backquote: "`",
    Minus: "-",
    Equal: "=",
  };
  if (named[trigger]) return named[trigger];
  if (/^Key[A-Z]$/.test(trigger)) return trigger.slice(3);
  if (/^Digit[0-9]$/.test(trigger)) return trigger.slice(5);
  if (/^Numpad/.test(trigger)) return `Numpad ${trigger.slice(6)}`;
  return trigger;
}
