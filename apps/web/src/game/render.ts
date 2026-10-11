// Canvas 2D text-cell renderer. Every world element is a glyph on a fixed
// monospace grid; walls are stroked so box joins stay seamless whatever the
// fallback font.
import { CELL, DIR_OFFSET, DIRS, type EntityView, type Tile } from "../protocol";
import type { Model } from "./model";
import { cellHash, shade, theme } from "./theme";

export interface RenderOptions {
  ascii: boolean;
  highContrast: boolean;
  reducedMotion: boolean;
  fontSize: number;
}

const TILE_CODE: Record<Tile, string> = {
  void: " ", wall: "#", floor: ".", road: ":", ash: ",", grass: '"', tree: "T", water: "~", bones: "b",
  rubble: "%", door: "+", shortcut: "D", fog_gate: "G", lantern: "L", fence: "|", stairs: ">",
  ember_pit: "^", altar: "A", false_wall: "X", grave: "t", pew: "=",
};

export const FONT_STACK = '"DejaVu Sans Mono", "Cascadia Mono", "Menlo", "Consolas", "Liberation Mono", monospace';

export class Renderer {
  private ctx: CanvasRenderingContext2D;
  cellW = 10;
  cellH = 18;
  cols = 0;
  rows = 0;
  private start = performance.now();

  constructor(private canvas: HTMLCanvasElement, public opts: RenderOptions) {
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) throw new Error("Canvas 2D is not available");
    this.ctx = ctx;
  }

  /** Fit the backing store to the element size and device pixel ratio. */
  resize() {
    const dpr = Math.max(1, Math.min(3, window.devicePixelRatio || 1));
    const rect = this.canvas.getBoundingClientRect();
    this.canvas.width = Math.max(1, Math.floor(rect.width * dpr));
    this.canvas.height = Math.max(1, Math.floor(rect.height * dpr));
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.ctx.font = `${this.opts.fontSize}px ${FONT_STACK}`;
    this.cellW = Math.ceil(this.ctx.measureText("M").width);
    this.cellH = Math.ceil(this.opts.fontSize * 1.25);
    this.cols = Math.floor(rect.width / this.cellW);
    this.rows = Math.floor(rect.height / this.cellH);
  }

  private pick(unicode: string, ascii: string) {
    return this.opts.ascii ? ascii : unicode;
  }

  private col(name: string, light = 1) {
    return shade(name, light, this.opts.highContrast);
  }

  private glyph(cx: number, cy: number, g: string, fg: string, bg?: string, bold = false) {
    const x = cx * this.cellW, y = cy * this.cellH;
    const ctx = this.ctx;
    if (bg) {
      ctx.fillStyle = bg;
      ctx.fillRect(x, y, this.cellW, this.cellH);
    }
    if (g === " ") return;
    ctx.fillStyle = fg;
    ctx.font = `${bold ? "bold " : ""}${this.opts.fontSize}px ${FONT_STACK}`;
    ctx.fillText(g, x + this.cellW / 2, y + this.cellH / 2 + 1);
  }

  private label(cx: number, cy: number, text: string, color: string, center = true) {
    const ctx = this.ctx;
    ctx.font = `${Math.round(this.opts.fontSize * 0.8)}px ${FONT_STACK}`;
    ctx.fillStyle = color;
    const w = ctx.measureText(text).width;
    const x = center ? cx * this.cellW + this.cellW / 2 - w / 2 : cx * this.cellW;
    ctx.textAlign = "left";
    ctx.fillText(text, x, cy * this.cellH + this.cellH / 2);
    ctx.textAlign = "center";
  }

  private wall(m: Model, mx: number, my: number, sx: number, sy: number, light: number) {
    const isWall = (x: number, y: number) => {
      const t = m.tile(x, y);
      return t === "wall" || (t === "false_wall" && m.doorClosed(x, y));
    };
    const edge = (x: number, y: number) => {
      if (!isWall(x, y)) return false;
      for (let dy = -1; dy <= 1; dy++)
        for (let dx = -1; dx <= 1; dx++) {
          const n = m.tile(x + dx, y + dy);
          if (n !== "wall" && n !== "void" && n !== "false_wall") return true;
        }
      return false;
    };
    const x = sx * this.cellW, y = sy * this.cellH;
    const ctx = this.ctx;
    ctx.fillStyle = this.col("soot", 0.45 + 0.55 * light);
    ctx.fillRect(x, y, this.cellW, this.cellH);
    if (!edge(mx, my)) return;
    if (this.opts.ascii) {
      this.glyph(sx, sy, "#", this.col("slate_hi", light));
      return;
    }
    const cxp = x + this.cellW / 2, cyp = y + this.cellH / 2;
    ctx.strokeStyle = this.col("slate_hi", light);
    ctx.lineWidth = Math.max(2, this.cellW * 0.28);
    ctx.lineCap = "square";
    ctx.beginPath();
    let any = false;
    const seg = (tx: number, ty: number) => {
      ctx.moveTo(cxp, cyp);
      ctx.lineTo(tx, ty);
      any = true;
    };
    if (edge(mx, my - 1)) seg(cxp, y);
    if (edge(mx + 1, my)) seg(x + this.cellW, cyp);
    if (edge(mx, my + 1)) seg(cxp, y + this.cellH);
    if (edge(mx - 1, my)) seg(x, cyp);
    if (!any) {
      ctx.fillStyle = this.col("slate_hi", light);
      ctx.fillRect(cxp - this.cellW * 0.3, cyp - this.cellW * 0.3, this.cellW * 0.6, this.cellW * 0.6);
    }
    ctx.stroke();
  }

  draw(m: Model, now: number) {
    const ctx = this.ctx;
    const motion = !this.opts.reducedMotion;
    const secs = (now - this.start) / 1000;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillStyle = this.col("void");
    ctx.fillRect(0, 0, this.cols * this.cellW + this.cellW, this.rows * this.cellH + this.cellH);
    const me = m.renderPos(m.me, now);
    if (!me) return;
    const pcx = Math.floor(me[0] / CELL), pcy = Math.floor(me[1] / CELL);
    const ox = pcx - Math.floor(this.cols / 2);
    const oy = pcy - Math.floor(this.rows / 2);
    const tele = m.telegraphCells(now);
    const L = theme.light;
    const lanterns = m.map.lanterns.map((l) => [l.x, l.y] as [number, number]);

    for (let sy = 0; sy < this.rows; sy++) {
      for (let sx = 0; sx < this.cols; sx++) {
        const cx = ox + sx, cy = oy + sy;
        const tile = m.tile(cx, cy);
        if (!m.isExplored(cx, cy) || tile === "void") continue;
        const visible = m.isVisible(cx, cy);
        let light = L.remembered;
        if (visible) {
          const d = Math.hypot(cx - pcx, cy - pcy);
          light = L.ambient + (1 - L.ambient) * Math.pow(Math.max(0, 1 - d / L.player_radius), 0.8);
          for (const [lx, ly] of lanterns) {
            const dl = Math.hypot(cx - lx, cy - ly);
            const flicker = motion ? 0.92 + 0.08 * Math.sin(secs * 7 + lx) : 1;
            light = Math.max(light, Math.max(0, 1 - dl / L.lantern_radius) * flicker);
          }
          light = Math.min(1, Math.max(L.ambient, light));
        }
        if (tile === "wall" || (tile === "false_wall" && m.doorClosed(cx, cy))) {
          this.wall(m, cx, cy, sx, sy, light);
          continue;
        }
        const style = theme.tiles[TILE_CODE[tile]];
        const h = cellHash(cx, cy);
        let fg = style.fg;
        let bg = style.bg;
        let g = this.pick(style.glyphs[h % style.glyphs.length], style.ascii);
        let tl = light;
        let bold = false;
        if (tile === "false_wall") g = this.pick("·", ".");
        if (tile === "door" || tile === "shortcut") g = m.doorClosed(cx, cy) ? this.pick(style.glyphs[0], style.ascii) : style.open ?? "'";
        if (tile === "lantern") {
          fg = motion && Math.sin(secs * 3 + cx) > 0.3 ? "amber_hi" : "amber";
          tl = 1;
        } else if (tile === "ember_pit" && motion) {
          const phase = (Math.sin(secs * 4 + h * 0.001) + 1) / 2;
          g = this.pick(phase > 0.5 ? "▒" : "░", "^");
          fg = phase > 0.7 ? "amber_hi" : "amber";
        } else if (tile === "fog_gate") {
          g = this.pick(!motion || Math.sin(secs * 2 + cy) > 0 ? "▒" : "░", "&");
          fg = m.fogSealed() ? "bone" : "fog_hi";
          tl = Math.max(tl, 0.7);
        } else if (tile === "water" && motion) {
          g = this.pick(Math.sin(Math.sin(secs * 1.5 + cx * 0.7) + cy) > 0 ? "≈" : "~", "~");
        }
        if (visible) {
          const tc = tele.get(`${cx},${cy}`);
          if (tc) {
            const ts = theme.telegraphs[tc[0]];
            const urgency = 1 - tc[1];
            g = this.pick(ts.glyph, ts.ascii);
            fg = ts.fg;
            bg = ts.bg;
            tl = 0.55 + 0.45 * urgency;
            bold = urgency > 0.75;
          }
        }
        this.glyph(sx, sy, g, this.col(fg, tl), this.col(bg, visible ? 0.45 + 0.55 * Math.min(1, tl) : L.remembered + 0.1), bold);
      }
    }

    if (motion) {
      for (const [lx, ly] of lanterns) {
        if (!m.isVisible(lx, ly)) continue;
        for (let i = 0; i < 3; i++) {
          const phase = (secs * 0.6 + i * 0.33 + lx * 0.1) % 1;
          const ex = lx + Math.round(Math.sin(secs * 1.3 + i * 2.1) * 1.5);
          const ey = ly - Math.floor(phase * 3) - 1;
          if (m.isVisible(ex, ey) && m.tile(ex, ey) !== "wall") this.glyph(ex - ox, ey - oy, this.pick("·", "."), this.col("amber_hi", 1 - phase * 0.6));
        }
      }
    }

    const ents = [...m.entities.values()].sort((a, b) => order(a.view, m.me) - order(b.view, m.me));
    for (const tr of ents) {
      const v = tr.view;
      const pos = m.renderPos(v.id, now);
      if (!pos) continue;
      const cx = Math.floor(pos[0] / CELL), cy = Math.floor(pos[1] / CELL);
      if (v.id !== m.me && v.rel !== "party" && !m.isVisible(cx, cy)) continue;
      const sx = cx - ox, sy = cy - oy;
      const flash = tr.flashUntil > now;
      this.entity(v, sx, sy, flash, m.me, secs, motion);
    }

    m.effects.forEach((fx, i) => {
      const spread = (i % 3) - 1;
      const age = Math.min(1, (now - fx.born) / fx.life);
      const cx = Math.floor(fx.x / CELL), cy = Math.floor(fx.y / CELL);
      const sx = cx - ox, sy = cy - oy;
      if (fx.kind === "number") {
        const rise = motion ? age * 2 : 0;
        const ctx2 = this.ctx;
        ctx2.font = `bold ${this.opts.fontSize}px ${FONT_STACK}`;
        ctx2.fillStyle = this.col(fx.color, 1 - age * 0.5);
        ctx2.fillText(fx.text, (sx + spread * 2) * this.cellW + this.cellW / 2, (sy - 1 - rise) * this.cellH + this.cellH / 2);
      } else if (fx.kind === "swing") {
        const idx = DIRS.indexOf(fx.facing);
        for (const k of [-1, 0, 1]) {
          const d = DIRS[(idx + k + 8) % 8];
          const [dx, dy] = DIR_OFFSET[d];
          if (m.tile(cx + dx, cy + dy) === "wall") continue;
          const g = d === "n" || d === "s" ? this.pick(fx.heavy ? "┃" : "│", "|")
            : d === "e" || d === "w" ? this.pick(fx.heavy ? "━" : "─", "-")
            : d === "ne" || d === "sw" ? this.pick("╱", "/") : this.pick("╲", "\\");
          this.glyph(sx + dx, sy + dy, g, this.col("bone", 1 - age * 0.6), undefined, true);
        }
      } else if (fx.kind === "burst") {
        const r = motion ? Math.floor(age * 2.5) + 1 : 1;
        DIRS.forEach((d, i) => {
          if (i % 2 === 1 && age > 0.5) return;
          const [dx, dy] = DIR_OFFSET[d];
          this.glyph(sx + dx * r, sy + dy * r, this.pick("·", "."), this.col(fx.color, 1 - age));
        });
      } else if (fx.kind === "ring") {
        const r = fx.radius * (motion ? age : 1);
        const steps = Math.max(8, Math.floor(r * 8));
        for (let i = 0; i < steps; i++) {
          const a = (i / steps) * Math.PI * 2;
          const rx = Math.round(Math.cos(a) * r), ry = Math.round(Math.sin(a) * r);
          if (m.isVisible(cx + rx, cy + ry)) this.glyph(sx + rx, sy + ry, this.pick("◌", "o"), this.col(fx.color, 1 - age * 0.7));
        }
      }
    });

    if (m.banner) {
      const b = m.banner;
      const ctx2 = this.ctx;
      const midX = (this.cols * this.cellW) / 2;
      const y = (this.rows * this.cellH) / 3;
      const size = Math.round(this.opts.fontSize * 2);
      ctx2.font = `bold ${size}px ${FONT_STACK}`;
      const text = this.opts.ascii ? b.text : b.text.split("").join(" ");
      const w = ctx2.measureText(text).width;
      ctx2.fillStyle = "rgba(11,10,9,0.72)";
      ctx2.fillRect(midX - w / 2 - 30, y - size, w + 60, size * 2 + (b.sub ? this.cellH : 0));
      ctx2.strokeStyle = this.col(b.color, 0.5);
      ctx2.lineWidth = 1;
      ctx2.beginPath();
      ctx2.moveTo(midX - w / 2 - 20, y - size * 0.75);
      ctx2.lineTo(midX + w / 2 + 20, y - size * 0.75);
      ctx2.moveTo(midX - w / 2 - 20, y + size * 0.75);
      ctx2.lineTo(midX + w / 2 + 20, y + size * 0.75);
      ctx2.stroke();
      ctx2.fillStyle = this.col(b.color);
      ctx2.fillText(text, midX, y);
      if (b.sub) {
        ctx2.font = `italic ${this.opts.fontSize}px ${FONT_STACK}`;
        ctx2.fillStyle = this.col("ash");
        ctx2.fillText(b.sub, midX, y + size);
      }
    }
  }

  private entity(v: EntityView, sx: number, sy: number, flash: boolean, me: number, secs: number, motion: boolean) {
    const E = theme.entities;
    const inv = (fg: string, bg?: string): [string, string | undefined] => (flash ? [this.col("void"), fg] : [fg, bg]);
    switch (v.kind) {
      case "player": {
        const look = v.look ?? { sigil: 0, hue: 0 };
        const i = look.sigil % E.player.glyphs.length;
        const g = v.state === "dead" ? "%" : this.pick(E.player.glyphs[i], E.player.ascii[i]);
        const hue = E.hues[look.hue] ?? "bone";
        const [fg, bg] = inv(this.col(hue), v.id === me ? this.col("slate_hi", 0.55) : undefined);
        this.glyph(sx, sy, g, fg, bg, true);
        if (v.rel === "party") {
          this.ctx.fillStyle = this.col("sickly_hi");
          this.ctx.fillRect(sx * this.cellW + 1, (sy + 1) * this.cellH - 2, this.cellW - 2, 2);
        }
        if (v.id !== me && v.name) this.label(sx, sy - 1, v.rel === "party" ? `${v.name}+` : v.name, this.col(v.rel === "party" ? "sickly_hi" : "ash"));
        if (v.state === "guarding") {
          const [dx, dy] = DIR_OFFSET[v.facing];
          this.glyph(sx + dx, sy + dy, this.pick(dx === 0 ? "─" : "│", "]"), this.col("stone"));
        }
        break;
      }
      case "npc": {
        const s = E.npc[v.def];
        this.glyph(sx, sy, s ? this.pick(s.glyph, s.ascii) : "&", this.col(s?.fg ?? "bone"), undefined, true);
        if (v.name) this.label(sx, sy - 1, v.name, this.col("gold"));
        break;
      }
      case "enemy": {
        const s = E.enemy[v.def];
        let g = s ? this.pick(s.glyph, s.ascii) : "e";
        let fgName = s?.fg ?? "blood_hi";
        if (v.state === "dead") [g, fgName] = ["%", "stone"];
        else if (v.state === "windup") fgName = "amber_hi";
        else if (v.state === "staggered") fgName = "fog_hi";
        const [fg, bg] = inv(this.col(fgName));
        this.glyph(sx, sy, g, fg, bg, true);
        if (v.state === "windup") this.glyph(sx, sy - 1, "!", this.col("amber_hi"), undefined, true);
        if (v.state === "staggered") this.glyph(sx, sy - 1, "?", this.col("fog_hi"));
        if (v.hp && v.hp[0] < v.hp[1] && v.state !== "dead") {
          const frac = v.hp[0] / v.hp[1];
          const x = (sx - 0.5) * this.cellW, y = (sy + 1) * this.cellH + 1;
          this.ctx.fillStyle = this.col("blood", 0.8);
          this.ctx.fillRect(x, y, this.cellW * 2, 3);
          this.ctx.fillStyle = this.col("blood_hi");
          this.ctx.fillRect(x, y, this.cellW * 2 * frac, 3);
        }
        break;
      }
      case "boss": {
        const b = E.boss[v.def];
        if (!b) break;
        const cracked = (v.phase ?? 1) >= 2;
        const rows = this.opts.ascii ? b.ascii : cracked ? b.sprite_cracked : b.sprite;
        const fgName = v.state === "windup" ? "amber_hi" : cracked && (!motion || Math.sin(secs * 5) > -0.3) ? b.phase2 : b.fg;
        rows.forEach((row, i) => {
          [...row].forEach((ch, j) => {
            const [fg, bg] = inv(this.col(fgName));
            this.glyph(sx - 1 + j, sy - 1 + i, ch, fg, bg, true);
          });
        });
        if (v.state === "windup") this.glyph(sx, sy - 2, "!", this.col("amber_hi"), undefined, true);
        break;
      }
      case "projectile": {
        const s = E.projectile[v.def] ?? E.projectile["default"];
        this.glyph(sx, sy, this.pick(s.glyph, s.ascii), this.col(s.fg), undefined, true);
        break;
      }
      case "loot": {
        const key = v.def.endsWith("_key") || v.def === "tallow_cup";
        const s = key ? E.key_loot : E.loot;
        const pulse = motion && Math.sin(secs * 3) > 0 ? "amber_hi" : s.fg;
        this.glyph(sx, sy, this.pick(s.glyph, s.ascii), this.col(pulse), undefined, true);
        break;
      }
      case "remnant": {
        const s = E.remnant;
        const pulse = motion && Math.sin(secs * 2) > 0 ? "sickly_hi" : "sickly";
        this.glyph(sx, sy, this.pick(s.glyph, s.ascii), this.col(pulse), undefined, true);
        break;
      }
    }
  }
}

function order(v: EntityView, me: number): number {
  switch (v.kind) {
    case "loot":
    case "remnant":
      return 0;
    case "npc":
      return 1;
    case "enemy":
    case "boss":
      return 2;
    case "projectile":
      return 3;
    case "player":
      return v.id === me ? 5 : 4;
  }
}
