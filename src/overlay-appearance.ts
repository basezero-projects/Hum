export type OverlayTextAppearanceInput = {
  autoContrast: boolean;
  surfaceIsLight: boolean | null;
  backgroundOwned: boolean;
  textColor: string;
  textColorDim: string;
};

export type OverlayTextAppearance = {
  autoColorActive: boolean;
  textColor: string;
  textColorDim: string;
  textShadow: string;
  useDarkLogo: boolean;
};

const LIGHT_TEXT_SHADOW =
  "0 1px 2px rgba(0,0,0,0.9), 0 3px 10px rgba(0,0,0,0.55)";
const DARK_TEXT_SHADOW =
  "0 1px 2px rgba(255,255,255,0.9), 0 3px 10px rgba(255,255,255,0.55)";

export function ownsReadableBackground(input: {
  backgroundHidden: boolean;
  blurVisible: boolean;
  opacityPct: number;
}): boolean {
  return (
    !input.backgroundHidden &&
    (input.blurVisible || input.opacityPct >= 75)
  );
}

export function resolveOverlayTextAppearance(
  input: OverlayTextAppearanceInput,
): OverlayTextAppearance {
  const autoColorActive = input.autoContrast && input.surfaceIsLight !== null;
  const useDarkText =
    autoColorActive && input.surfaceIsLight === true && input.backgroundOwned;

  if (!autoColorActive) {
    return {
      autoColorActive: false,
      textColor: input.textColor,
      textColorDim: input.textColorDim,
      textShadow: LIGHT_TEXT_SHADOW,
      useDarkLogo: false,
    };
  }

  return {
    autoColorActive: true,
    textColor: useDarkText ? "#0a0a0a" : "#ffffff",
    textColorDim: useDarkText ? "#5a5a5a" : "#c8c8c8",
    textShadow: useDarkText ? DARK_TEXT_SHADOW : LIGHT_TEXT_SHADOW,
    useDarkLogo: useDarkText,
  };
}
