// Plain floor routing and bounded layout refinement; no rendering dependencies.
import {
  PROCESS_SIDE,
  PROCESS_FRAME_SIDE,
  PARENT_LINE_WIDTH,
} from "./model.js";
import type { ParentRouteInfo, Position, TreePosition } from "./types.js";

type Point = Pick<Position, "x" | "y">;
/** Includes the selected frame, round stroke radius, and free floor clearance. */
const FAMILY_OBSTACLE_HALF =
  PROCESS_FRAME_SIDE / 2 + PARENT_LINE_WIDTH / 2 + 0.2;
const STEP = PROCESS_SIDE / 4;
const EPS = 1e-8;
const CELL = STEP * 16;

/** Closed axis-aligned intersection, including corners and round-cap clearance. */
function familySegmentIntersects(a: Point, b: Point, obstacle: Point) {
  const h = FAMILY_OBSTACLE_HALF;
  return (
    Math.max(a.x, b.x) >= obstacle.x - h - EPS &&
    Math.min(a.x, b.x) <= obstacle.x + h + EPS &&
    Math.max(a.y, b.y) >= obstacle.y - h - EPS &&
    Math.min(a.y, b.y) <= obstacle.y + h + EPS
  );
}

class ObstacleIndex {
  private cells = new Map<string, Array<[string, Point]>>();
  readonly xSteps = new Map<number, [number | undefined, number | undefined]>();
  readonly ySteps = new Map<number, [number | undefined, number | undefined]>();
  readonly cachedPorts = new Map<string, Port[]>();
  private edgeClear = new Map<string, boolean>();
  constructor(readonly positions: ReadonlyMap<string, Point>) {
    for (const [id, p] of positions) {
      const h = FAMILY_OBSTACLE_HALF;
      for (
        let x = Math.floor((p.x - h) / CELL);
        x <= Math.floor((p.x + h) / CELL);
        x++
      )
        for (
          let y = Math.floor((p.y - h) / CELL);
          y <= Math.floor((p.y + h) / CELL);
          y++
        ) {
          const cell = `${x}:${y}`;
          if (!this.cells.has(cell)) this.cells.set(cell, []);
          this.cells.get(cell)!.push([id, p]);
        }
    }
    // Contract empty lattice runs. Turns remain on the .55 grid at endpoint
    // exits and just outside obstacle boundaries; long edges are checked whole.
    for (const [axis, steps] of [
      ["x", this.xSteps],
      ["y", this.ySteps],
    ] as const) {
      const coordinates = new Set<number>();
      for (const p of positions.values()) {
        coordinates.add(Math.round(p[axis] / STEP));
        coordinates.add(
          Math.floor((p[axis] - FAMILY_OBSTACLE_HALF - STEP) / STEP),
        );
        coordinates.add(
          Math.ceil((p[axis] + FAMILY_OBSTACLE_HALF + STEP) / STEP),
        );
      }
      const sorted = [...coordinates].sort((a, b) => a - b);
      if (sorted.length) {
        sorted.unshift(sorted[0] - 1);
        sorted.push(sorted.at(-1)! + 1);
      }
      sorted.forEach((value, i) =>
        steps.set(value, [sorted[i - 1], sorted[i + 1]]),
      );
    }
  }
  gridClear(a: Point, b: Point) {
    const id =
      a.x < b.x || (a.x === b.x && a.y < b.y)
        ? `${a.x}:${a.y}:${b.x}:${b.y}`
        : `${b.x}:${b.y}:${a.x}:${a.y}`;
    let clear = this.edgeClear.get(id);
    if (clear === undefined) {
      clear = !this.blockers(
        { x: a.x * STEP, y: a.y * STEP },
        { x: b.x * STEP, y: b.y * STEP },
      ).size;
      this.edgeClear.set(id, clear);
    }
    return clear;
  }
  blockers(a: Point, b: Point, ignored: ReadonlySet<string> = new Set()) {
    const found = new Set<string>();
    for (
      let x = Math.floor(Math.min(a.x, b.x) / CELL);
      x <= Math.floor(Math.max(a.x, b.x) / CELL);
      x++
    )
      for (
        let y = Math.floor(Math.min(a.y, b.y) / CELL);
        y <= Math.floor(Math.max(a.y, b.y) / CELL);
        y++
      )
        for (const [id, p] of this.cells.get(`${x}:${y}`) ?? [])
          if (!ignored.has(id) && familySegmentIntersects(a, b, p))
            found.add(id);
    return found;
  }
  clear(points: readonly Point[], ignored: ReadonlySet<string> = new Set()) {
    return points
      .slice(1)
      .every((b, i) => !this.blockers(points[i], b, ignored).size);
  }
}

function midpointRoute(
  parent: string,
  child: string,
  positions: ReadonlyMap<string, Point>,
): ParentRouteInfo {
  const a = positions.get(parent)!,
    b = positions.get(child)!;
  const y = (a.y + b.y) / 2;
  return {
    parent,
    child,
    points: [
      { x: a.x, y: a.y },
      { x: a.x, y },
      { x: b.x, y },
      { x: b.x, y: b.y },
    ],
  };
}

interface Port {
  x: number;
  y: number;
  points: Point[];
  axis: number;
  cost: number;
}
function ports(id: string, index: ObstacleIndex): Port[] {
  const cached = index.cachedPorts.get(id);
  if (cached) return cached;
  const p = index.positions.get(id)!,
    h = FAMILY_OBSTACLE_HALF;
  const result: Port[] = [];
  for (const [dx, dy] of [
    [0, 1],
    [1, 0],
    [0, -1],
    [-1, 0],
  ]) {
    const x = dx
      ? dx > 0
        ? Math.ceil((p.x + h + STEP) / STEP)
        : Math.floor((p.x - h - STEP) / STEP)
      : Math.round(p.x / STEP);
    const y = dy
      ? dy > 0
        ? Math.ceil((p.y + h + STEP) / STEP)
        : Math.floor((p.y - h - STEP) / STEP)
      : Math.round(p.y / STEP);
    const end = { x: x * STEP, y: y * STEP };
    const points = [p, dx ? { x: end.x, y: p.y } : { x: p.x, y: end.y }, end];
    if (index.clear(points, new Set([id]))) {
      const tangent =
        Math.abs(points[1].x - end.x) + Math.abs(points[1].y - end.y) > EPS;
      result.push({
        x,
        y,
        points,
        axis: tangent ? (dx ? 1 : 0) : dx ? 0 : 1,
        cost: distance(points) + (tangent ? PROCESS_SIDE : 0),
      });
    }
  }
  index.cachedPorts.set(id, result);
  return result;
}

interface SearchState {
  x: number;
  y: number;
  axis: number;
  cost: number;
  score: number;
  serial: number;
  previous?: SearchState;
  port: Port;
}
class SearchQueue {
  private values: SearchState[] = [];
  private before(a: SearchState, b: SearchState) {
    return (
      a.score < b.score ||
      (a.score === b.score &&
        (a.cost > b.cost || (a.cost === b.cost && a.serial < b.serial)))
    );
  }
  push(value: SearchState) {
    const a = this.values;
    let i = a.length;
    a.push(value);
    while (i) {
      const parent = (i - 1) >> 1;
      if (!this.before(value, a[parent])) break;
      a[i] = a[parent];
      i = parent;
    }
    a[i] = value;
  }
  pop() {
    const a = this.values,
      first = a[0],
      last = a.pop();
    if (a.length && last) {
      let i = 0;
      while (i * 2 + 1 < a.length) {
        let next = i * 2 + 1;
        if (next + 1 < a.length && this.before(a[next + 1], a[next])) next++;
        if (!this.before(a[next], last)) break;
        a[i] = a[next];
        i = next;
      }
      a[i] = last;
    }
    return first;
  }
}

function distance(points: readonly Point[]) {
  return points
    .slice(1)
    .reduce(
      (sum, b, i) =>
        sum + Math.abs(points[i].x - b.x) + Math.abs(points[i].y - b.y),
      0,
    );
}
function simplify(points: Point[]) {
  const result: Point[] = [];
  for (const p of points) {
    const b = result.at(-1),
      a = result.at(-2);
    if (b && b.x === p.x && b.y === p.y) continue;
    if (
      a &&
      b &&
      ((a.x === b.x && b.x === p.x) || (a.y === b.y && b.y === p.y))
    )
      result.pop();
    result.push({ x: p.x, y: p.y });
  }
  return result;
}

function detours(
  parent: string,
  children: string[],
  index: ObstacleIndex,
): Map<string, Point[]> {
  const starts = ports(parent, index);
  const ends = children.flatMap((child) =>
    ports(child, index).map((p) => ({ ...p, child })),
  );
  const batch = children.length > 1;
  if (!starts.length || !ends.length)
    throw new Error(
      `No free family route endpoint: ${parent} → ${children.join(", ")}`,
    );
  const allPorts = [...starts, ...ends];
  const xs = allPorts.map((p) => p.x),
    ys = allPorts.map((p) => p.y);
  const outer = {
    minX: Math.min(...index.xSteps.keys()),
    maxX: Math.max(...index.xSteps.keys()),
    minY: Math.min(...index.ySteps.keys()),
    maxY: Math.max(...index.ySteps.keys()),
  };
  // Small searches usually suffice; the final search includes an outside corridor.
  for (const margin of batch ? [Infinity] : [8, 24, Infinity]) {
    const bounds =
      margin === Infinity
        ? outer
        : {
            minX: Math.min(...xs) - margin,
            maxX: Math.max(...xs) + margin,
            minY: Math.min(...ys) - margin,
            maxY: Math.max(...ys) + margin,
          };
    const queue = new SearchQueue(),
      best = new Map<string, number>();
    const targets = new Map<string, typeof ends>();
    for (const p of ends) {
      const id = `${p.x}:${p.y}`;
      if (!targets.has(id)) targets.set(id, []);
      targets.get(id)!.push(p);
    }
    const stateKey = (p: Pick<SearchState, "x" | "y" | "axis">) =>
      `${p.x}:${p.y}:${p.axis}`;
    let serial = 0;
    const heuristic = (x: number, y: number, axis: number) => {
      if (batch) return 0; // One shared shortest-path tree for a large fanout.
      let minimum = Infinity;
      for (const p of ends) {
        const dx = Math.abs(x - p.x),
          dy = Math.abs(y - p.y),
          targetAxis = p.axis;
        const bends =
          dx && dy
            ? axis === targetAxis
              ? 2
              : 1
            : dx || dy
              ? Number(axis !== (dx ? 0 : 1)) +
                Number(targetAxis !== (dx ? 0 : 1))
              : Number(axis !== targetAxis);
        minimum = Math.min(
          minimum,
          (dx + dy) * STEP + p.cost + bends * PROCESS_SIDE,
        );
      }
      return minimum;
    };
    for (const port of starts) {
      const { axis, cost } = port;
      const state = {
        ...port,
        port,
        axis,
        cost,
        score: cost + heuristic(port.x, port.y, axis),
        serial: serial++,
      };
      best.set(stateKey(state), cost);
      queue.push(state);
    }
    const winners = new Map<
      string,
      { state: SearchState; port: Port; cost: number }
    >();
    let cutoff = Infinity;
    let state: SearchState | undefined;
    while ((state = queue.pop())) {
      if (state.score >= cutoff - EPS) break;
      if (state.cost > best.get(stateKey(state))! + EPS) continue;
      for (const target of targets.get(`${state.x}:${state.y}`) ?? []) {
        const axis = target.axis;
        const cost =
          state.cost + target.cost + (state.axis === axis ? 0 : PROCESS_SIDE);
        if (cost < (winners.get(target.child)?.cost ?? Infinity)) {
          winners.set(target.child, { state, port: target, cost });
          if (winners.size === children.length)
            cutoff = Math.max(...[...winners.values()].map((w) => w.cost));
        }
      }
      const [left, right] = index.xSteps.get(state.x)!;
      const [down, up] = index.ySteps.get(state.y)!;
      for (const [x, y, axis] of [
        [state.x, up, 1],
        [right, state.y, 0],
        [state.x, down, 1],
        [left, state.y, 0],
      ] as const) {
        if (x === undefined || y === undefined) continue;
        if (
          x < bounds.minX ||
          x > bounds.maxX ||
          y < bounds.minY ||
          y > bounds.maxY
        )
          continue;
        const cost =
          state.cost +
          (Math.abs(x - state.x) + Math.abs(y - state.y)) * STEP +
          (axis === state.axis ? 0 : PROCESS_SIDE);
        const next = {
          x,
          y,
          axis,
          cost,
          score: cost + heuristic(x, y, axis),
          serial: serial++,
          previous: state,
          port: state.port,
        };
        if (cost >= (best.get(stateKey(next)) ?? Infinity) - EPS) continue;
        if (!index.gridClear(state, next)) continue;
        best.set(stateKey(next), cost);
        queue.push(next);
      }
    }
    if (winners.size === children.length) {
      return new Map(
        [...winners].map(([child, winner]) => {
          const middle: Point[] = [];
          for (let s: SearchState | undefined = winner.state; s; s = s.previous)
            middle.push({ x: s.x * STEP, y: s.y * STEP });
          return [
            child,
            simplify([
              ...winner.state.port.points,
              ...middle.reverse(),
              ...[...winner.port.points].reverse(),
            ]),
          ];
        }),
      );
    }
  }
  throw new Error(
    `No unobstructed family route: ${parent} → ${children.join(", ")}`,
  );
}

/** Order is independent of input order; missing parents and self links are omitted. */
export function familyRoutes(
  positions: ReadonlyMap<string, TreePosition>,
): ParentRouteInfo[] {
  const index = new ObstacleIndex(positions);
  const routes: ParentRouteInfo[] = [];
  const blocked = new Map<string, ParentRouteInfo[]>();
  for (const [child, p] of [...positions].sort(([a], [b]) =>
    a.localeCompare(b),
  )) {
    const parent = p.parent;
    if (!parent || parent === child || !positions.has(parent)) continue;
    const route = midpointRoute(parent, child, positions);
    routes.push(route);
    if (index.clear(route.points, new Set([parent, child]))) continue;
    if (!blocked.has(parent)) blocked.set(parent, []);
    blocked.get(parent)!.push(route);
  }
  for (const [parent, group] of blocked) {
    if (group.length > 16) {
      const paths = detours(
        parent,
        group.map((r) => r.child),
        index,
      );
      for (const route of group) route.points = paths.get(route.child)!;
    } else {
      for (const route of group)
        route.points = detours(parent, [route.child], index).get(route.child)!;
    }
  }
  return routes;
}

function collisions(positions: ReadonlyMap<string, TreePosition>) {
  const index = new ObstacleIndex(positions),
    blockers = new Set<string>();
  let count = 0;
  for (const [child, p] of positions) {
    if (!p.parent || !positions.has(p.parent)) continue;
    const route = midpointRoute(p.parent, child, positions),
      ignored = new Set([p.parent, child]);
    for (let i = 1; i < route.points.length; i++)
      for (const id of index.blockers(
        route.points[i - 1],
        route.points[i],
        ignored,
      )) {
        count++;
        blockers.add(id);
      }
  }
  return { count, blockers };
}

/** Initial layout/Rearrange only: move at most eight blockers once, by ≤one side. */
export function refineFamilyLayout(positions: Map<string, TreePosition>) {
  const moved = new Set<string>();
  let current = collisions(positions);
  while (current.count && moved.size < 8) {
    const entries = [...positions];
    const originalIndex = new ObstacleIndex(positions);
    const originalRoutes = entries.flatMap(([child, q]) =>
      q.parent && positions.has(q.parent)
        ? [midpointRoute(q.parent, child, positions)]
        : [],
    );
    const originalSections = originalRoutes.flatMap((route) =>
      route.points.slice(1).map((b, i) => ({
        parent: route.parent,
        child: route.child,
        a: route.points[i],
        b,
      })),
    );
    let winner:
      { id: string; y: number; count: number; shift: number } | undefined;
    for (const id of [...current.blockers].sort()) {
      if (moved.has(id)) continue;
      const p = positions.get(id)!,
        oldY = p.y;
      const affected = originalRoutes.filter(
        (route) => route.parent === id || route.child === id,
      );
      const affectedCount = () =>
        affected.reduce((sum, old) => {
          const route = midpointRoute(old.parent, old.child, positions);
          const ignored = new Set([route.parent, route.child]);
          return (
            sum +
            route.points
              .slice(1)
              .reduce(
                (n, b, i) =>
                  n + originalIndex.blockers(route.points[i], b, ignored).size,
                0,
              )
          );
        }, 0);
      const blockingCount = () => {
        let count = 0;
        for (const section of originalSections)
          if (
            section.parent !== id &&
            section.child !== id &&
            familySegmentIntersects(section.a, section.b, p)
          )
            count++;
        return count;
      };
      const remaining = current.count - affectedCount() - blockingCount();
      for (const units of [-1, 1, -2, 2, -3, 3, -4, 4]) {
        const y = oldY + units * STEP;
        if (p.parent && positions.get(p.parent)!.y >= y) continue;
        if (
          entries.some(
            ([other, q]) =>
              other !== id &&
              ((q.parent === id && q.y <= y) ||
                (Math.abs(q.x - p.x) < PROCESS_FRAME_SIDE - EPS &&
                  Math.abs(q.y - y) < PROCESS_FRAME_SIDE - EPS)),
          )
        )
          continue;
        p.y = y;
        const count = remaining + affectedCount() + blockingCount();
        p.y = oldY;
        const shift = Math.abs(units * STEP);
        if (count >= current.count) continue;
        if (
          !winner ||
          count < winner.count ||
          (count === winner.count &&
            (shift < winner.shift ||
              (shift === winner.shift && id < winner.id)))
        )
          winner = { id, y, count, shift };
      }
    }
    if (!winner) break;
    positions.get(winner.id)!.y = winner.y;
    moved.add(winner.id);
    current = collisions(positions);
  }
}
