// Sample visible geometry and activity only when a tour shot is chosen.
import type { RenderView } from "./contracts.js";
import type { CpuGlow, TourScene } from "./types.js";
import { key } from "./model.js";

export function tourScene(
  view: RenderView,
  cpu: ReadonlyMap<string, CpuGlow>,
  now: number,
): TourScene {
  const degrees = new Map<string, number>();
  for (const { e } of view.edgeViews) {
    for (const endpoint of [e.endpoint, e.peer]) {
      if (!endpoint) continue;
      const id = key(endpoint.process_id);
      degrees.set(id, (degrees.get(id) ?? 0) + 1);
    }
  }
  const curves = [
    ...view.edgeViews,
    ...view.networkViews,
    ...view.fileViews.values(),
    ...view.netlinkViews.values(),
  ];
  return {
    subjects: view.hullIds.flatMap((id) => {
      const node = view.nodes.get(id);
      if (!node) return [];
      const glow = cpu.get(id);
      return [
        {
          id,
          position: { x: node.pos.x, y: node.pos.y, z: node.pos.z + 4 },
          activity:
            glow && now >= glow.last && now - glow.last < 5000
              ? glow.runtime_ns / Math.max(1, glow.window_ms * 1e6)
              : 0,
          connections: degrees.get(id) ?? 0,
        },
      ];
    }),
    connections: curves.map(({ curve }) => curve.getPoints(4)),
  };
}
