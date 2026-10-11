// Per-browser display preferences. Nothing security-relevant lives here.
export interface Settings {
  ascii: boolean;
  highContrast: boolean;
  reducedMotion: boolean;
  fontSize: number;
}

const KEY = "hollowrite.settings";

export function loadSettings(): Settings {
  const prefersReduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const defaults: Settings = { ascii: false, highContrast: false, reducedMotion: prefersReduced, fontSize: 16 };
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return defaults;
    const v = JSON.parse(raw);
    return {
      ascii: !!v.ascii,
      highContrast: !!v.highContrast,
      reducedMotion: !!v.reducedMotion,
      fontSize: Math.min(26, Math.max(11, Number(v.fontSize) || 16)),
    };
  } catch {
    return defaults;
  }
}

export function saveSettings(s: Settings) {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    // storage may be unavailable (private mode); preferences then last for the tab only
  }
}
