import type { BuildInfo } from "./types";

export type WindowSurface =
  | "overlay"
  | "settings"
  | "activation"
  | "setup"
  | "dev_console";

type BuildInfoLoader = () => Promise<BuildInfo>;

function customerWindowSurface(
  label: string | null,
): Exclude<WindowSurface, "dev_console"> | null {
  if (label === "overlay") return "overlay";
  if (label === "settings") return "settings";
  if (label === "activation") return "activation";
  if (label === "setup") return "setup";
  return null;
}

export async function resolveWindowSurface(
  label: string | null,
  loadBuildInfo: BuildInfoLoader,
): Promise<WindowSurface> {
  const customerSurface = customerWindowSurface(label);
  if (customerSurface) return customerSurface;

  try {
    const buildInfo = await loadBuildInfo();
    return buildInfo.developer_console === true ? "dev_console" : "settings";
  } catch {
    return "settings";
  }
}
