import raw from "@content/palettes/theme.json";

export interface TileStyle { glyphs: string[]; ascii: string; open?: string; fg: string; bg: string }
export interface GlyphStyle { glyph: string; ascii: string; fg: string }
export interface BossStyle { fg: string; phase2: string; sprite: string[]; sprite_cracked: string[]; ascii: string[] }
export interface TeleStyle { glyph: string; ascii: string; fg: string; bg: string }

interface RawTheme {
  palette: Record<string, string>;
  tiles: Record<string, TileStyle>;
  walls: { heavy: Record<string, string>; solid: string };
  entities: {
    player: { glyphs: string[]; ascii: string[] };
    hues: string[];
    npc: Record<string, GlyphStyle>;
    enemy: Record<string, GlyphStyle>;
    boss: Record<string, BossStyle>;
    projectile: Record<string, GlyphStyle>;
    loot: GlyphStyle;
    key_loot: GlyphStyle;
    remnant: GlyphStyle;
  };
  telegraphs: Record<string, TeleStyle>;
  ui: Record<string, string>;
  light: { player_radius: number; lantern_radius: number; ambient: number; remembered: number };
}

export const theme = raw as unknown as RawTheme;

const rgbCache = new Map<string, [number, number, number]>();

export function rgb(name: string): [number, number, number] {
  let v = rgbCache.get(name);
  if (!v) {
    const hex = (theme.palette[name] ?? "#c8c8c8").slice(1);
    const n = parseInt(hex, 16);
    v = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    rgbCache.set(name, v);
  }
  return v;
}

export function shade(name: string, light: number, highContrast = false): string {
  const l = Math.min(1, Math.max(0, highContrast ? light * 1.35 : light));
  const [r, g, b] = rgb(name);
  return `rgb(${Math.round(r * l)},${Math.round(g * l)},${Math.round(b * l)})`;
}

export function uiColor(key: string): string {
  return shade(theme.ui[key] ?? "bone", 1);
}

/** Same hash as the terminal client so both render identical glyph variation. */
export function cellHash(x: number, y: number): number {
  let h = Math.imul(x >>> 0, 374761393) ^ Math.imul(y >>> 0, 668265263);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return (h ^ (h >>> 16)) >>> 0;
}
