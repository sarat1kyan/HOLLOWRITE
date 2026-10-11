import { describe, expect, it } from "vitest";
import { Model, Tracked } from "./model";
import type { CharacterSheet, EntityView, MapData } from "../protocol";

const map: MapData = {
  id: "m", name: "m", width: 7, height: 3,
  rows: ["#######", "#..D..#", "#######"],
  zones: [], lanterns: [], doors: [{ id: "d", x: 3, y: 1 }],
};

const sheet = { open_doors: [] as string[] } as unknown as CharacterSheet;

describe("client model", () => {
  it("closed doors block sight and movement until opened", () => {
    const m = new Model(1, map, { ...sheet, open_doors: [] });
    m.updateFov([1, 1]);
    expect(m.isVisible(3, 1)).toBe(true);
    expect(m.isVisible(5, 1)).toBe(false);
    expect(m.passable(3, 1)).toBe(false);
    m.sheet = { ...m.sheet, open_doors: ["d"] };
    m.updateFov([1, 1]);
    expect(m.isVisible(5, 1)).toBe(true);
    expect(m.passable(3, 1)).toBe(true);
  });

  it("interpolates between samples and clamps outside them", () => {
    const t = new Tracked({ id: 1, x: 0, y: 0 } as EntityView);
    t.push(0, 0, 0);
    t.push(100, 1000, 0);
    expect(t.posAt(50)[0]).toBe(500);
    expect(t.posAt(5000)[0]).toBe(1000);
    expect(t.posAt(-50)[0]).toBe(0);
  });

  it("prediction never walks into walls", () => {
    const m = new Model(1, map, { ...sheet, open_doors: [] });
    m.prediction = { x: 1500, y: 1500, history: [] };
    for (let i = 0; i < 60; i++) m.predict(i * 33, 33, "e");
    expect(m.prediction!.x).toBeLessThan(3000 - 300 + 1);
    for (let i = 0; i < 60; i++) m.predict(i * 33, 33, "n");
    expect(m.prediction!.y).toBeGreaterThanOrEqual(1300);
  });
});
