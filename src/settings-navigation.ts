export type SettingsPageId =
  | "general"
  | "timing"
  | "text"
  | "background"
  | "layout"
  | "features"
  | "shortcuts"
  | "streaming"
  | "about";

export type SettingsPage = {
  id: SettingsPageId;
  number: string;
  label: string;
  title: string;
  description: string;
};

export const SETTINGS_PAGES: ReadonlyArray<SettingsPage> = [
  {
    id: "general",
    number: "01",
    label: "General",
    title: "General",
    description: "Choose how Hum starts and how the overlay behaves when it opens.",
  },
  {
    id: "timing",
    number: "02",
    label: "Timing",
    title: "Lyrics timing",
    description: "Match the words to headphones, speakers, Bluetooth, and each media source.",
  },
  {
    id: "text",
    number: "03",
    label: "Text",
    title: "Text style",
    description: "Set the type, size, weight, color, and alignment of every lyric line.",
  },
  {
    id: "background",
    number: "04",
    label: "Background",
    title: "Background",
    description: "Control the surface, artwork atmosphere, transparency, and Windows backdrop.",
  },
  {
    id: "layout",
    number: "05",
    label: "Layout",
    title: "Layout",
    description: "Choose the overlay shape, lyric density, and space between lines.",
  },
  {
    id: "features",
    number: "06",
    label: "Features",
    title: "Features",
    description: "Choose which supporting details and artist tools appear around the lyrics.",
  },
  {
    id: "shortcuts",
    number: "07",
    label: "Shortcuts",
    title: "Global shortcuts",
    description: "Put every global action on a key or mouse button that feels natural to you.",
  },
  {
    id: "streaming",
    number: "08",
    label: "Streaming",
    title: "OBS and streaming",
    description: "Expose a local browser source for a clean lyrics layer in your scenes.",
  },
  {
    id: "about",
    number: "09",
    label: "About",
    title: "About and support",
    description: "Manage your license, updates, support tools, diagnostics, and app data.",
  },
];

export function availableSettingsPages(globalShortcuts: boolean): ReadonlyArray<SettingsPage> {
  return globalShortcuts
    ? SETTINGS_PAGES
    : SETTINGS_PAGES.filter(({ id }) => id !== "shortcuts");
}

export function normalizeSettingsPage(
  candidate: string | null | undefined,
  globalShortcuts: boolean,
): SettingsPageId {
  const pages = availableSettingsPages(globalShortcuts);
  return pages.some(({ id }) => id === candidate) ? (candidate as SettingsPageId) : "general";
}
