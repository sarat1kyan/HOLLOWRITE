import { dirFromOffset, DIR_OFFSET, type Action, type ClientMsg, type Dir8 } from "../protocol";

const MOVE: Record<string, Dir8> = {
  KeyW: "n", ArrowUp: "n", KeyS: "s", ArrowDown: "s", KeyA: "w", ArrowLeft: "w", KeyD: "e", ArrowRight: "e",
  KeyQ: "nw", KeyE: "ne", KeyZ: "sw", KeyC: "se",
};

const ACTIONS: Record<string, Action> = {
  KeyJ: "light", KeyK: "heavy", Space: "roll", KeyI: "parry", KeyH: "heal", Digit1: "cast1", Digit2: "cast2", KeyF: "interact",
};

export const HEARTBEAT_MS = 100;

/** Browsers deliver real key-up events, so movement is plain held state. */
export class Input {
  private held = new Set<string>();
  private guard = false;
  private seq = 0;
  private lastSent = 0;
  private lastDir: Dir8 | null = null;

  constructor(private send: (m: ClientMsg) => void) {}

  dir(): Dir8 | null {
    let dx = 0, dy = 0;
    for (const code of this.held) {
      const [x, y] = DIR_OFFSET[MOVE[code]];
      dx += x;
      dy += y;
    }
    return dirFromOffset(Math.max(-1, Math.min(1, dx)), Math.max(-1, Math.min(1, dy)));
  }

  private emit(act?: Action) {
    this.seq += 1;
    const mv = this.dir();
    this.lastDir = mv;
    this.lastSent = performance.now();
    const msg: ClientMsg = { t: "input", seq: this.seq };
    if (mv) msg.mv = mv;
    if (act) msg.act = act;
    this.send(msg);
  }

  /** Returns true when the key was consumed by gameplay. */
  keydown(e: KeyboardEvent): boolean {
    if (e.ctrlKey || e.metaKey || e.altKey) return false;
    if (MOVE[e.code]) {
      if (!this.held.has(e.code)) {
        this.held.add(e.code);
        this.emit();
      }
      return true;
    }
    if (e.code === "KeyL") {
      if (!this.guard) {
        this.guard = true;
        this.emit("guard_on");
      }
      return true;
    }
    const act = ACTIONS[e.code];
    if (act) {
      if (!e.repeat) this.emit(act);
      return true;
    }
    return false;
  }

  keyup(e: KeyboardEvent): boolean {
    if (MOVE[e.code]) {
      this.held.delete(e.code);
      this.emit();
      return true;
    }
    if (e.code === "KeyL" && this.guard) {
      this.guard = false;
      this.emit("guard_off");
      return true;
    }
    return false;
  }

  /** Drop everything held, e.g. when focus moves to a text field. */
  release() {
    const hadGuard = this.guard;
    this.held.clear();
    this.guard = false;
    this.emit(hadGuard ? "guard_off" : undefined);
  }

  tick(now: number) {
    const d = this.dir();
    if (d !== this.lastDir || (d && now - this.lastSent >= HEARTBEAT_MS)) this.emit();
  }
}
