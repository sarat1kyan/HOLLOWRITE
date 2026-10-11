import { useEffect, useRef } from "react";
import { CELL, type Tile } from "../protocol";
import type { Model } from "../game/model";
import { shade } from "../game/theme";

const COLOURS: Partial<Record<Tile, string>> = {
  wall: "slate", false_wall: "slate", tree: "moss", water: "water", lantern: "amber_hi",
  ember_pit: "rust", fog_gate: "fog_hi", door: "rust", shortcut: "rust",
};

export function MapView({ model }: { model: Model }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current!;
    const scale = 7;
    canvas.width = model.width * scale;
    canvas.height = model.height * scale;
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = shade("void", 1);
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    for (let y = 0; y < model.height; y++) {
      for (let x = 0; x < model.width; x++) {
        const t = model.tile(x, y);
        if (!model.isExplored(x, y) || t === "void") continue;
        const c = COLOURS[t];
        ctx.fillStyle = c ? shade(c, 1) : shade("stone", 0.55);
        ctx.fillRect(x * scale, y * scale, scale, scale);
      }
    }
    ctx.font = "12px monospace";
    for (const l of model.map.lanterns) {
      if (!model.isExplored(l.x, l.y)) continue;
      const known = model.sheet.lanterns.includes(l.id);
      ctx.fillStyle = shade(known ? "amber_hi" : "stone", 1);
      ctx.fillText(`☼ ${l.name}`, l.x * scale + scale + 2, l.y * scale + scale);
    }
    const rem = model.meView?.remnant;
    if (rem) {
      ctx.fillStyle = shade("sickly_hi", 1);
      ctx.fillText("◊", (rem[0] / CELL) * scale - 3, (rem[1] / CELL) * scale + 4);
    }
    const p = model.renderPos(model.me, performance.now());
    if (p) {
      ctx.fillStyle = shade("bone", 1);
      ctx.beginPath();
      ctx.arc((p[0] / CELL) * scale, (p[1] / CELL) * scale, scale * 0.8, 0, Math.PI * 2);
      ctx.fill();
    }
  }, [model]);
  return <canvas ref={ref} style={{ width: "100%", imageRendering: "pixelated" }} aria-label="Explored areas of Veyr, with lanterns and your position" />;
}
