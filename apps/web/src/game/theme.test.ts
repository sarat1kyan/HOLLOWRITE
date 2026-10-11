import { describe, expect, it } from "vitest";
import { cellHash, theme } from "./theme";

describe("shared theme", () => {
  it("hashes cells exactly like the terminal client", () => {
    // Reference values computed by crates/terminal-client/src/theme.rs::cell_hash.
    const reference: [number, number, number][] = [
      [0, 0, 0], [1, 2, 2135657881], [127, 51, 4028803565], [-3, 7, 1347741029], [64, 23, 3481691865],
    ];
    for (const [x, y, h] of reference) expect(cellHash(x, y)).toBe(h);
  });

  it("styles every tile code", () => {
    for (const code of [" ", "#", ".", ":", ",", '"', "T", "~", "b", "%", "+", "D", "G", "L", "|", ">", "^", "A", "X", "t", "="]) {
      expect(theme.tiles[code], code).toBeDefined();
    }
  });

  it("keeps ascii fallbacks ascii", () => {
    for (const t of Object.values(theme.tiles)) expect(/^[\x20-\x7e]$/.test(t.ascii)).toBe(true);
    for (const t of Object.values(theme.telegraphs)) expect(/^[\x20-\x7e]$/.test(t.ascii)).toBe(true);
  });
});
