import { describe, expect, it } from "vitest";
import { Input } from "./input";
import type { ClientMsg } from "../protocol";

const key = (code: string, repeat = false) => ({ code, repeat, ctrlKey: false, metaKey: false, altKey: false }) as KeyboardEvent;

describe("browser input", () => {
  it("combines held keys into diagonals and releases cleanly", () => {
    const sent: ClientMsg[] = [];
    const input = new Input((m) => sent.push(m));
    input.keydown(key("KeyW"));
    input.keydown(key("KeyD"));
    expect(input.dir()).toBe("ne");
    input.keyup(key("KeyW"));
    expect(input.dir()).toBe("e");
    input.keyup(key("KeyD"));
    expect(input.dir()).toBeNull();
    const seqs = sent.map((m) => (m.t === "input" ? m.seq : -1));
    expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
    expect(new Set(seqs).size).toBe(seqs.length);
  });

  it("ignores key repeat for actions and pairs guard on/off", () => {
    const sent: ClientMsg[] = [];
    const input = new Input((m) => sent.push(m));
    input.keydown(key("KeyJ"));
    input.keydown(key("KeyJ", true));
    input.keydown(key("KeyL"));
    input.keydown(key("KeyL", true));
    input.keyup(key("KeyL"));
    const acts = sent.map((m) => (m.t === "input" ? m.act : undefined)).filter(Boolean);
    expect(acts).toEqual(["light", "guard_on", "guard_off"]);
  });
});
