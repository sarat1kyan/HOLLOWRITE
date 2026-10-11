// Client-side world view: interpolation, local movement prediction, field of
// view and transient effects. Mirrors crates/terminal-client/src/model.rs.
import {
  CELL, DIR_OFFSET, DIRS, cleanText, opaque, tileFromCode, walkable,
  type ActorState, type CharacterSheet, type ChatChannel, type Dir8, type EntityId, type EntityView,
  type GameEvent, type MapData, type PartyMember, type SelfView, type Snapshot, type Telegraph,
  type TelegraphKind, type Tile,
} from "../protocol";

export const INTERP_DELAY = 110;
export const FOV_RADIUS = 18;

interface Sample { at: number; x: number; y: number }

export class Tracked {
  view: EntityView;
  samples: Sample[] = [];
  flashUntil = 0;

  constructor(view: EntityView) {
    this.view = view;
  }

  push(at: number, x: number, y: number) {
    this.samples.push({ at, x, y });
    if (this.samples.length > 8) this.samples.shift();
  }

  posAt(t: number): [number, number] {
    let prev: Sample | null = null;
    for (const s of this.samples) {
      if (s.at >= t) {
        if (!prev) return [s.x, s.y];
        const span = Math.max(1, s.at - prev.at);
        const k = Math.min(1, Math.max(0, (t - prev.at) / span));
        return [prev.x + (s.x - prev.x) * k, prev.y + (s.y - prev.y) * k];
      }
      prev = s;
    }
    return prev ? [prev.x, prev.y] : [this.view.x, this.view.y];
  }
}

export type Effect =
  | { kind: "number"; text: string; color: string; x: number; y: number; born: number; life: number }
  | { kind: "swing"; facing: Dir8; heavy: boolean; x: number; y: number; born: number; life: number }
  | { kind: "burst"; color: string; x: number; y: number; born: number; life: number }
  | { kind: "ring"; radius: number; color: string; x: number; y: number; born: number; life: number };

export interface Banner { text: string; sub: string; color: string; until: number }
export interface ChatLine { channel: ChatChannel; from: string; text: string; at: number }
export interface Dialogue { npc: EntityId; speaker: string; lines: string[]; choices: string[] }

interface Prediction { x: number; y: number; history: { at: number; x: number; y: number }[] }

const TELE_RANK: Record<TelegraphKind, number> = { shockwave: 5, heavy: 4, area: 3, strike: 2, hazard: 1, weak: 0 };

export class Model {
  me: EntityId;
  map: MapData;
  tiles: Tile[];
  width: number;
  height: number;
  doors = new Map<string, string>();
  sheet: CharacterSheet;
  meView: SelfView | null = null;
  entities = new Map<EntityId, Tracked>();
  tele: { t: Telegraph; at: number }[] = [];
  tick = 0;
  ack = 0;
  lastSnapshot = performance.now();
  explored: Uint8Array;
  visible: Uint8Array;
  private fovFrom: string | null = null;
  private fovDoors = -1;
  effects: Effect[] = [];
  toasts: { text: string; at: number }[] = [];
  banner: Banner | null = null;
  chat: ChatLine[] = [];
  party: PartyMember[] = [];
  invites: string[] = [];
  dialogue: Dialogue | null = null;
  prediction: Prediction | null = null;
  rtt: number | null = null;
  zoneName = "";
  version = 0;

  constructor(
    me: EntityId, map: MapData, sheet: CharacterSheet,
    public walkSpeed = 120, public bodyRadius = 300, public tickHz = 30,
  ) {
    this.me = me;
    this.map = map;
    this.sheet = sheet;
    this.width = map.width;
    this.height = map.height;
    this.tiles = map.rows.flatMap((r) => [...r].map(tileFromCode));
    for (const d of map.doors) this.doors.set(`${d.x},${d.y}`, d.id);
    this.explored = new Uint8Array(this.width * this.height);
    this.visible = new Uint8Array(this.width * this.height);
  }

  tile(x: number, y: number): Tile {
    if (x < 0 || y < 0 || x >= this.width || y >= this.height) return "void";
    return this.tiles[y * this.width + x];
  }

  idx(x: number, y: number): number {
    return x >= 0 && y >= 0 && x < this.width && y < this.height ? y * this.width + x : -1;
  }

  doorClosed(x: number, y: number): boolean {
    const id = this.doors.get(`${x},${y}`);
    return id !== undefined && !this.sheet.open_doors.includes(id);
  }

  fogSealed(): boolean {
    return !!this.meView?.boss;
  }

  passable(x: number, y: number): boolean {
    const t = this.tile(x, y);
    if (t === "door" || t === "shortcut" || t === "false_wall") return !this.doorClosed(x, y);
    if (t === "fog_gate") return !this.fogSealed();
    return walkable(t);
  }

  opaqueAt(x: number, y: number): boolean {
    const t = this.tile(x, y);
    if (t === "door" || t === "shortcut" || t === "false_wall") return this.doorClosed(x, y);
    return opaque(t);
  }

  myState(): ActorState {
    return this.meView?.state ?? "idle";
  }

  applySnapshot(s: Snapshot, now = performance.now()) {
    this.tick = s.tick;
    this.ack = s.ack;
    this.lastSnapshot = now;
    for (const id of s.gone ?? []) this.entities.delete(id);
    for (const e of s.entities) {
      let tr = this.entities.get(e.id);
      if (!tr) {
        tr = new Tracked(e);
        this.entities.set(e.id, tr);
      }
      tr.view = e;
      tr.push(now, e.x, e.y);
    }
    this.tele = (s.tele ?? []).map((t) => ({ t, at: now }));
    const zone = this.map.zones.slice().reverse().find((z) => z.id === s.me.zone);
    if (zone) this.zoneName = zone.name;
    this.meView = s.me;
    this.reconcile(now);
    this.version++;
  }

  private reconcile(now: number) {
    const me = this.entities.get(this.me);
    if (!me) return;
    const sx = me.view.x, sy = me.view.y;
    const free = ["idle", "moving", "guarding", "healing"].includes(this.myState());
    if (!free) {
      this.prediction = null;
      return;
    }
    if (!this.prediction) {
      this.prediction = { x: sx, y: sy, history: [] };
      return;
    }
    const p = this.prediction;
    const oneWay = this.rtt === null ? 40 : this.rtt / 2;
    const target = now - oneWay - 33;
    let past: [number, number] = [p.x, p.y];
    for (let i = p.history.length - 1; i >= 0; i--) {
      if (p.history[i].at <= target) {
        past = [p.history[i].x, p.history[i].y];
        break;
      }
    }
    const ex = sx - past[0], ey = sy - past[1];
    const err = Math.hypot(ex, ey);
    if (err > 1800) {
      p.x = sx;
      p.y = sy;
      p.history = [];
    } else {
      const k = err > 400 ? 0.5 : 0.2;
      p.x += ex * k;
      p.y += ey * k;
      for (const h of p.history) {
        h.x += ex * k;
        h.y += ey * k;
      }
    }
  }

  predict(now: number, dtMs: number, held: Dir8 | null) {
    const state = this.myState();
    const pct = state === "idle" || state === "moving" ? 100 : state === "guarding" ? 50 : state === "healing" ? 40 : 0;
    const p = this.prediction;
    if (!p) return;
    if (held && pct > 0) {
      const perSec = (this.walkSpeed * this.tickHz * pct) / 100;
      const dist = perSec * Math.min(0.1, dtMs / 1000);
      const [dx, dy] = DIR_OFFSET[held];
      const norm = dx !== 0 && dy !== 0 ? Math.SQRT1_2 : 1;
      const mx = dx * dist * norm, my = dy * dist * norm;
      if (this.boxClear(p.x + mx, p.y)) p.x += mx;
      if (this.boxClear(p.x, p.y + my)) p.y += my;
    }
    p.history.push({ at: now, x: p.x, y: p.y });
    if (p.history.length > 120) p.history.shift();
  }

  private boxClear(x: number, y: number): boolean {
    const r = this.bodyRadius;
    const pts: [number, number][] = [[x - r, y - r], [x + r, y - r], [x - r, y + r], [x + r, y + r]];
    return pts.every(([cx, cy]) => this.passable(Math.floor(cx / CELL), Math.floor(cy / CELL)));
  }

  renderPos(id: EntityId, now: number): [number, number] | null {
    if (id === this.me && this.prediction) return [this.prediction.x, this.prediction.y];
    const t = this.entities.get(id);
    if (!t) return null;
    return t.posAt(now - (id === this.me ? 30 : INTERP_DELAY));
  }

  myCell(now: number): [number, number] | null {
    const p = this.renderPos(this.me, now);
    return p ? [Math.floor(p[0] / CELL), Math.floor(p[1] / CELL)] : null;
  }

  updateFov(from: [number, number]) {
    const key = `${from[0]},${from[1]}`;
    const doors = this.sheet.open_doors.length;
    if (this.fovFrom === key && this.fovDoors === doors) return;
    this.fovFrom = key;
    this.fovDoors = doors;
    this.visible.fill(0);
    const r = FOV_RADIUS;
    for (let i = -r; i <= r; i++) {
      this.cast(from, [from[0] + i, from[1] - r], r);
      this.cast(from, [from[0] + i, from[1] + r], r);
      this.cast(from, [from[0] - r, from[1] + i], r);
      this.cast(from, [from[0] + r, from[1] + i], r);
    }
  }

  private cast(a: [number, number], b: [number, number], r: number) {
    let [x, y] = a;
    const dx = Math.abs(b[0] - a[0]);
    const dy = -Math.abs(b[1] - a[1]);
    const sx = a[0] < b[0] ? 1 : -1;
    const sy = a[1] < b[1] ? 1 : -1;
    let err = dx + dy;
    for (;;) {
      const ddx = x - a[0], ddy = y - a[1];
      if (ddx * ddx + ddy * ddy > r * r) return;
      const i = this.idx(x, y);
      if (i < 0) return;
      this.visible[i] = 1;
      this.explored[i] = 1;
      if ((x !== a[0] || y !== a[1]) && this.opaqueAt(x, y)) return;
      if (x === b[0] && y === b[1]) return;
      const e2 = 2 * err;
      if (e2 >= dy) {
        err += dy;
        x += sx;
      }
      if (e2 <= dx) {
        err += dx;
        y += sy;
      }
    }
  }

  isVisible(x: number, y: number): boolean {
    const i = this.idx(x, y);
    return i >= 0 && this.visible[i] === 1;
  }

  isExplored(x: number, y: number): boolean {
    const i = this.idx(x, y);
    return i >= 0 && this.explored[i] === 1;
  }

  toast(text: string) {
    this.toasts.push({ text, at: performance.now() });
    if (this.toasts.length > 5) this.toasts.shift();
    this.version++;
  }

  setBanner(text: string, sub: string, color: string, secs: number) {
    this.banner = { text, sub, color, until: performance.now() + secs * 1000 };
  }

  pushChat(channel: ChatChannel, from: string, text: string) {
    this.chat.push({ channel, from: cleanText(from), text: cleanText(text), at: Date.now() });
    if (this.chat.length > 200) this.chat.shift();
    this.version++;
  }

  private fx(e: Effect) {
    this.effects.push(e);
  }

  applyEvents(events: GameEvent[]) {
    const now = performance.now();
    for (const e of events) {
      switch (e.e) {
        case "hit": {
          let text = String(e.amount);
          let color = e.dst === this.me ? "blood_hi" : "bone";
          if (e.kind === "dodged") [text, color] = ["dodge", "fog_hi"];
          else if (e.kind === "parried") [text, color] = ["PARRY", "amber_hi"];
          else if (e.kind === "blocked") color = "stone";
          else if (e.kind === "critical") [text, color] = [`${e.amount}!`, "amber_hi"];
          else if (e.kind === "hazard") color = "sickly_hi";
          this.fx({ kind: "number", text, color, x: e.x, y: e.y, born: now, life: 900 });
          const t = this.entities.get(e.dst);
          if (t && e.amount > 0) t.flashUntil = now + 120;
          break;
        }
        case "swing": {
          const heavy = ["heavy", "slam", "lunge"].includes(e.action);
          this.fx({ kind: "swing", facing: e.facing, heavy, x: e.x, y: e.y, born: now, life: heavy ? 260 : 180 });
          break;
        }
        case "staggered": {
          const t = this.entities.get(e.id);
          if (t) this.fx({ kind: "number", text: "staggered", color: "amber", x: t.view.x, y: t.view.y - 600, born: now, life: 700 });
          break;
        }
        case "died":
          this.fx({ kind: "burst", color: "bone", x: e.x, y: e.y, born: now, life: 700 });
          if (e.id === this.me) this.setBanner("UNTHREADED", "your echoes stay where you fell", "blood_hi", 3.5);
          break;
        case "respawned":
          if (e.id === this.me) {
            this.prediction = null;
            this.setBanner("THE LANTERN REMEMBERS YOU", "", "amber", 2.2);
          }
          break;
        case "echoes_gained": this.toast(`+${e.amount} echoes`); break;
        case "echoes_lost": this.toast(`${e.amount} echoes are lost for good`); break;
        case "echoes_reclaimed": this.setBanner("ECHOES RECLAIMED", `${e.amount} return to you`, "sickly_hi", 2.5); break;
        case "item_gained": this.toast(e.qty > 1 ? `Found ${e.name} x${e.qty}` : `Found ${e.name}`); break;
        case "lantern_lit": this.setBanner("LANTERN LIT", e.name, "amber_hi", 2.8); break;
        case "rested": this.toast("You rest. Health, draughts and wick restored."); break;
        case "level_up": this.toast(`You feel more yourself. Level ${e.level}.`); break;
        case "boss_phase": this.setBanner("THE BELL CRACKS", "", "sickly_hi", 2.5); break;
        case "boss_defeated": this.setBanner(`${e.name.toUpperCase()} SILENCED`, "", "gold", 4.5); break;
        case "boss_reset": this.toast("The belfry falls silent and waits."); break;
        case "toll": this.fx({ kind: "ring", radius: e.radius / 10, color: "bone", x: e.x, y: e.y, born: now, life: 900 }); break;
        case "door_opened": this.toast("A way opens."); break;
        case "notice": this.toast(cleanText(e.text)); break;
        case "zone_entered": this.setBanner(e.name, "", "fog_hi", 2.0); break;
      }
    }
    this.version++;
  }

  prune(now: number) {
    this.effects = this.effects.filter((e) => now - e.born < e.life);
    const before = this.toasts.length;
    this.toasts = this.toasts.filter((t) => now - t.at < 6000);
    if (this.banner && now > this.banner.until) this.banner = null;
    if (before !== this.toasts.length) this.version++;
  }

  telegraphCells(now: number): Map<string, [TelegraphKind, number]> {
    const out = new Map<string, [TelegraphKind, number]>();
    const tickMs = 1000 / this.tickHz;
    for (const { t, at } of this.tele) {
      const left = Math.max(0, t.left - (now - at) / tickMs);
      const frac = t.total === 0 ? 0 : Math.min(1, Math.max(0, left / t.total));
      for (const [x, y] of t.cells) {
        const key = `${x},${y}`;
        const cur = out.get(key);
        if (!cur || TELE_RANK[t.kind] > TELE_RANK[cur[0]]) out.set(key, [t.kind, frac]);
      }
    }
    return out;
  }
}

export function dirName(d: Dir8): string {
  return d.toUpperCase();
}

export { DIRS };
