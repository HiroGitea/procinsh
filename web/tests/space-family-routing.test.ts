import { test } from "vitest";
import assert from "node:assert/strict";
import {
  familyRoutes,
  refineFamilyLayout,
} from "../src/space/family-routing.js";
import { SpaceDataStore } from "../src/space/data.js";
import type { ParentRouteInfo, TreePosition } from "../src/space/types.js";
import { processInfo } from "./support/fixtures.js";

function layout(values: Array<[string, number, number, string | null]>) {
  return new Map(
    values.map(([id, x, y, parent]) => [id, { x, y, parent, depth: 0 }]),
  );
}

/** Independent footprint oracle: every stroke/cap keeps .2 clearance from frames. */
function assertClear(
  routes: readonly ParentRouteInfo[],
  positions: ReadonlyMap<string, Pick<TreePosition, "x" | "y">>,
) {
  for (const route of routes) {
    assert.deepEqual(route.points[0], {
      x: positions.get(route.parent)!.x,
      y: positions.get(route.parent)!.y,
    });
    assert.deepEqual(route.points.at(-1), {
      x: positions.get(route.child)!.x,
      y: positions.get(route.child)!.y,
    });
    for (let i = 1; i < route.points.length; i++) {
      const a = route.points[i - 1],
        b = route.points[i];
      assert.ok(a.x === b.x || a.y === b.y, "orthogonal sections");
      const minX = Math.min(a.x, b.x),
        maxX = Math.max(a.x, b.x),
        minY = Math.min(a.y, b.y),
        maxY = Math.max(a.y, b.y);
      for (const [id, p] of positions) {
        if (id === route.parent || id === route.child) continue;
        const dx = Math.max(minX - (p.x + 1.225), p.x - 1.225 - maxX, 0);
        const dy = Math.max(minY - (p.y + 1.225), p.y - 1.225 - maxY, 0);
        if (dx * dx + dy * dy <= (0.5 - 1e-8) ** 2)
          assert.fail(`${route.parent} → ${route.child} crosses ${id}`);
      }
    }
  }
}

test("preserves clear midpoint routes and detours horizontal, vertical and corner blockers", () => {
  const plain = layout([
    ["p", 0, 0, null],
    ["c", 20, 20, "p"],
  ]);
  assert.deepEqual(familyRoutes(plain)[0].points, [
    { x: 0, y: 0 },
    { x: 0, y: 10 },
    { x: 20, y: 10 },
    { x: 20, y: 20 },
  ]);
  for (const obstacle of [
    [10, 10],
    [0, 5],
    [20, 15],
    [10, 11.5],
    [0, 10],
  ]) {
    const positions = new Map(plain);
    positions.set("obstacle", {
      x: obstacle[0],
      y: obstacle[1],
      parent: null,
      depth: 0,
    });
    const routes = familyRoutes(positions);
    assert.notDeepEqual(routes[0].points, familyRoutes(plain)[0].points);
    assertClear(routes, positions);
    assert.deepEqual(
      familyRoutes(new Map([...positions].reverse())),
      routes,
      "input order does not change routes",
    );
  }
});

test("routes around multiple blockers and a wall using outside corridors", () => {
  const positions = layout([
    ["p", -8, 0, null],
    ["c", 8, 0, "p"],
  ]);
  for (let i = -10; i <= 10; i++)
    positions.set(`wall${i}`, { x: 0, y: i * 3.3, parent: null, depth: 0 });
  const routes = familyRoutes(positions);
  assertClear(routes, positions);
  assert.ok(
    routes[0].points.some((p) => Math.abs(p.y) > 34),
    "escapes endpoint search area",
  );
});

test("omits missing and self parents and terminates on cyclic input", () => {
  const positions = layout([
    ["a", 0, 0, "b"],
    ["b", 10, 10, "a"],
    ["self", 30, 0, "self"],
    ["orphan", 40, 0, "missing"],
  ]);
  const routes = familyRoutes(positions);
  assert.equal(routes.length, 2);
  assertClear(routes, positions);
});

test("local refinement reduces obstruction without box overlap and stays bounded", () => {
  const original = layout([
    ["p", 0, 0, null],
    ["c", 20, 20, "p"],
    ["blocker", 10, 10, null],
  ]);
  const positions = new Map([...original].map(([id, p]) => [id, { ...p }]));
  refineFamilyLayout(positions);
  assert.equal(Math.abs(positions.get("blocker")!.y - 10), 2.2);
  const repeated = new Map(
    [...original].reverse().map(([id, p]) => [id, { ...p }]),
  );
  refineFamilyLayout(repeated);
  assert.deepEqual([...repeated].sort(), [...positions].sort());
  let moved = 0;
  for (const [id, p] of positions) {
    assert.equal(p.x, original.get(id)!.x);
    if (p.y !== original.get(id)!.y) moved++;
    assert.ok(Math.abs(p.y - original.get(id)!.y) <= 2.2 + 1e-8);
    for (const [other, q] of positions)
      if (id !== other)
        assert.ok(Math.abs(p.x - q.x) >= 2.45 || Math.abs(p.y - q.y) >= 2.45);
  }
  assert.ok(moved <= 8);
  assert.equal(
    familyRoutes(positions)[0].points.length,
    4,
    "local move clears the original path",
  );
});

function process(pid: number, parent: number | null) {
  return processInfo({
    identity: { pid, start_time_ticks: 1 },
    parent_id: parent === null ? null : { pid: parent, start_time_ticks: 1 },
  });
}

test("refinement stops after eight nodes and routing handles remaining obstructions", () => {
  const positions = new Map<string, TreePosition>();
  for (let i = 0; i < 10; i++) {
    positions.set(`p${i}`, { x: i * 30, y: 0, parent: null, depth: 0 });
    positions.set(`c${i}`, {
      x: i * 30 + 20,
      y: 20,
      parent: `p${i}`,
      depth: 1,
    });
    positions.set(`b${i}`, { x: i * 30 + 10, y: 10, parent: null, depth: 0 });
  }
  refineFamilyLayout(positions);
  assert.equal(
    [...positions].filter(([id, p]) => id.startsWith("b") && p.y !== 10).length,
    8,
  );
  for (const [id, p] of positions)
    if (p.parent) assert.ok(positions.get(p.parent)!.y < p.y, id);
  assertClear(familyRoutes(positions), positions);
});

test("a thousand children added live remain safely routed", () => {
  const store = new SpaceDataStore();
  store.replaceSnapshot({ processes: [process(1, null)], fd_relations: [] });
  store.replaceSnapshot({
    processes: Array.from({ length: 1000 }, (_, i) =>
      process(i + 1, i ? 1 : null),
    ),
    fd_relations: [],
  });
  assert.equal(store.parentRoutes.length, 999);
  assertClear(store.parentRoutes, storePositions(store));
}, 20000);
function storePositions(store: SpaceDataStore) {
  return new Map([...store.nodes].map(([id, node]) => [id, { ...node.pos }]));
}

test("live wrapped children avoid existing boxes, retain coordinates and reuse routes", () => {
  const store = new SpaceDataStore();
  store.replaceSnapshot({ processes: [process(1, null)], fd_relations: [] });
  store.replaceSnapshot({
    processes: Array.from({ length: 80 }, (_, i) =>
      process(i + 1, i ? 1 : null),
    ),
    fd_relations: [],
  });
  const positions = storePositions(store),
    routes = store.parentRoutes;
  assertClear(routes, positions);
  store.replaceSnapshot({
    ...store.snapshot,
    processes: store.snapshot.processes
      .map((p) => ({ ...p, name: "renamed" }))
      .reverse(),
  });
  assert.equal(
    store.parentRoutes,
    routes,
    "metadata-only update reuses cached polylines",
  );
  assert.deepEqual(storePositions(store), new Map([...positions].reverse()));
  store.replaceSnapshot({
    ...store.snapshot,
    processes: [...store.snapshot.processes, process(81, 1)],
  });
  for (const [id, p] of positions)
    assert.deepEqual(
      store.nodes.get(id)!.pos,
      p,
      "newcomers never shift existing identities",
    );
  assert.notEqual(store.parentRoutes, routes);
  assertClear(store.parentRoutes, storePositions(store));
  store.replaceSnapshot({
    ...store.snapshot,
    processes: store.snapshot.processes.map((p) =>
      p.identity.pid === 2
        ? { ...p, parent_id: { pid: 3, start_time_ticks: 1 } }
        : p,
    ),
  });
  assert.ok(
    store.parentRoutes.some((r) => r.child === "2:1" && r.parent === "3:1"),
  );
  assertClear(store.parentRoutes, storePositions(store));
});

test("initial forest and Rearrange route a crowded hierarchy safely", () => {
  const store = new SpaceDataStore();
  const processes = Array.from({ length: 400 }, (_, i) =>
    process(i + 1, i ? Math.floor((i - 1) / 4) + 1 : null),
  );
  store.replaceSnapshot({ processes, fd_relations: [] });
  assertClear(store.parentRoutes, storePositions(store));
  const before = storePositions(store),
    routes = store.parentRoutes;
  store.replaceSnapshot(store.snapshot, true);
  assert.deepEqual(storePositions(store), before);
  assert.equal(
    store.parentRoutes,
    routes,
    "unchanged Rearrange reuses deterministic routes",
  );
});
