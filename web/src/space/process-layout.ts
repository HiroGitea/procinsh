// Pure process placement: full forest layout and stable incremental updates.
// Exports ProcessLayoutNode, treeLayout, and stableLayout; no SSE or rendering state.
import { key, PROCESS_SIDE } from "./model.js";
import type { ProcessId } from "../shared/api-types.js";
import type { Position, TreePosition } from "./types.js";

/** The identity and parent relationship needed to place a process. */
export interface ProcessLayoutNode {
  identity: ProcessId;
  parent_id?: ProcessId | null;
}
interface PackedTree {
  positions: Map<string, TreePosition>;
  width: number;
  height: number;
}

const WRAPPED_ROW_OFFSET = PROCESS_SIDE;

/**
 * Place a process forest on the x/y plane, centered horizontally.
 * Missing parents and self-parent links become roots; cycles are broken
 * deterministically. Keys include both PID and process start time.
 * Sibling subtrees stay side by side; only trees with different roots can wrap
 * onto another row. Within each tree, y coordinates follow tree depth.
 * xGap and yGap control horizontal and vertical spacing.
 */
export function treeLayout(
  processes: ProcessLayoutNode[],
  xGap = 4.8,
  yGap = 6.5,
): Map<string, TreePosition> {
  const ordered = [...processes].sort(
    (a, b) =>
      a.identity.pid - b.identity.pid ||
      a.identity.start_time_ticks - b.identity.start_time_ticks,
  );
  const known = new Map(ordered.map((node) => [key(node.identity), node]));
  const rank = new Map(
    ordered.map((node, index) => [key(node.identity), index]),
  );
  const parents = new Map<string, string | null>();
  for (const node of ordered) {
    const id = key(node.identity),
      parent = node.parent_id && key(node.parent_id);
    parents.set(
      id,
      parent && parent !== id && known.has(parent) ? parent : null,
    );
  }

  // A corrupt or racing proc snapshot must not make the layout recurse forever.
  for (const node of ordered) {
    let id: string | null | undefined = key(node.identity);
    const path: string[] = [],
      seen = new Map<string, number>();
    while (id && parents.has(id)) {
      if (seen.has(id)) {
        const cycle = path.slice(seen.get(id));
        const root = cycle.reduce((a, b) =>
          rank.get(a)! < rank.get(b)! ? a : b,
        );
        parents.set(root, null);
        break;
      }
      seen.set(id, path.length);
      path.push(id);
      id = parents.get(id);
    }
  }

  const children = new Map<string, string[]>(
    ordered.map((node) => [key(node.identity), []]),
  );
  for (const [id, parent] of parents)
    if (parent) children.get(parent)!.push(id);
  for (const list of children.values())
    list.sort((a, b) => rank.get(a)! - rank.get(b)!);
  const roots = ordered
    .map((node) => key(node.identity))
    .filter((id) => !parents.get(id));
  const pack = (
    layouts: PackedTree[],
    gap: number,
    wrapRows: boolean,
  ): PackedTree => {
    if (!layouts.length) return { positions: new Map(), width: 0, height: 0 };
    const area = layouts.reduce(
      (sum, layout) => sum + (layout.width + gap) * (layout.height + gap),
      0,
    );
    const target = wrapRows
      ? Math.max(
          40,
          ...layouts.map((layout) => layout.width),
          Math.sqrt(area) * 1.4,
        )
      : Infinity;
    const positions = new Map<string, TreePosition>();
    let cursorX = 0,
      cursorY = 0,
      rowHeight = 0,
      rowOffset = 0,
      width = 0;
    for (const layout of layouts) {
      if (cursorX > rowOffset && cursorX - rowOffset + layout.width > target) {
        // Move each wrapped forest row together with all descendants of its roots.
        rowOffset += WRAPPED_ROW_OFFSET;
        cursorX = rowOffset;
        cursorY += rowHeight + gap;
        rowHeight = 0;
      }
      for (const [id, position] of layout.positions)
        positions.set(id, {
          ...position,
          x: position.x + cursorX,
          y: position.y + cursorY,
        });
      cursorX += layout.width + gap;
      width = Math.max(width, cursorX - gap);
      rowHeight = Math.max(rowHeight, layout.height);
    }
    return { positions, width, height: cursorY + rowHeight };
  };
  const build = (id: string): PackedTree => {
    const packed = pack(children.get(id)!.map(build), 3, false);
    const width = Math.max(xGap, packed.width),
      offsetX = (width - packed.width) / 2;
    const positions = new Map<string, TreePosition>([
      [id, { x: width / 2, y: 0, depth: 0, parent: parents.get(id)! }],
    ]);
    for (const [child, position] of packed.positions)
      positions.set(child, {
        ...position,
        x: position.x + offsetX,
        y: position.y + yGap,
        depth: position.depth + 1,
      });
    return {
      positions,
      width,
      height: packed.height ? packed.height + yGap : yGap,
    };
  };
  const forest = pack(roots.map(build), 9, true),
    result = forest.positions;
  const values = [...result.values()];
  const center = values.length
    ? (Math.min(...values.map((v) => v.x)) +
        Math.max(...values.map((v) => v.x))) /
      2
    : 0;
  for (const value of values) value.x -= center;
  return result;
}

/**
 * Preserve x/y coordinates of live identities and place newcomers in vacant
 * space near their parent. Without surviving previous positions, use treeLayout.
 * Parent and depth metadata always describe the current normalized forest.
 * Inputs and previous coordinates are not mutated.
 */
export function stableLayout(
  processes: ProcessLayoutNode[],
  previous: ReadonlyMap<string, Pick<Position, "x" | "y">> = new Map(),
  xGap = 4.8,
  yGap = 6.5,
): Map<string, TreePosition> {
  const initial = treeLayout(processes, xGap, yGap),
    result = new Map<string, TreePosition>();
  for (const [id, place] of initial) {
    const old = previous.get(id);
    if (old) result.set(id, { ...place, x: old.x, y: old.y });
  }
  if (!result.size) return initial;
  const buckets = new Map<string, TreePosition[]>(),
    cell = (x: number, y: number) => `${x}:${y}`;
  const occupy = (place: TreePosition) => {
    const id = cell(Math.floor(place.x / xGap), Math.floor(place.y / yGap));
    if (!buckets.has(id)) buckets.set(id, []);
    buckets.get(id)!.push(place);
  };
  const vacant = (x: number, y: number) => {
    const cx = Math.floor(x / xGap),
      cy = Math.floor(y / yGap);
    for (let dx = -1; dx <= 1; dx++)
      for (let dy = -1; dy <= 1; dy++) {
        for (const p of buckets.get(cell(cx + dx, cy + dy)) || []) {
          if (
            Math.abs(p.x - x) < xGap - 1e-9 &&
            Math.abs(p.y - y) < yGap - 1e-9
          )
            return false;
        }
      }
    return true;
  };
  for (const place of result.values()) occupy(place);
  const existing = [...result.values()];
  const outside = {
    x: Math.max(...existing.map((p) => p.x)) + xGap,
    y: Math.min(...existing.map((p) => p.y)),
  };
  // Normalized tree depth also gives a finite, deterministic order for cycles.
  const newcomers = [...initial]
    .filter(([id]) => !result.has(id))
    .sort(
      (a, b) =>
        a[1].depth - b[1].depth ||
        Number(a[0].split(":")[0]) - Number(b[0].split(":")[0]) ||
        a[0].localeCompare(b[0]),
    );
  for (const [id, place] of newcomers) {
    const parent = place.parent ? result.get(place.parent) : undefined;
    const origin = parent ? { x: parent.x, y: parent.y + yGap } : outside;
    let found = null;
    // Expand square rings below the anchor, keeping children below their parent.
    for (let radius = 0; !found; radius++) {
      for (let dy = 0; dy <= radius && !found; dy++)
        for (let dx = -radius; dx <= radius; dx++) {
          if (Math.max(Math.abs(dx), dy) !== radius) continue;
          const x = origin.x + dx * xGap + dy * WRAPPED_ROW_OFFSET,
            y = origin.y + dy * yGap;
          if (vacant(x, y)) {
            found = { ...place, x, y };
            break;
          }
        }
    }
    result.set(id, found);
    occupy(found);
  }
  return result;
}

if (import.meta.vitest) {
  const { test } = import.meta.vitest;
  const processNode = (pid: number, parent: number | null = null) => ({
    identity: { pid, start_time_ticks: pid * 10 },
    parent_id:
      parent !== null ? { pid: parent, start_time_ticks: parent * 10 } : null,
  });

  test("subtrees stay side by side and only root trees wrap", async () => {
    const assert: typeof import("node:assert/strict") = (
      await import("node:assert/strict")
    ).default;
    const nodes = [processNode(1)];
    const branches: string[][] = [];
    for (let i = 0; i < 12; i++) {
      const child = processNode(i + 2, 1);
      nodes.push(child);
      const branch = [key(child.identity)];
      for (let j = 0; j < i + 5; j++) {
        const grandchild = processNode(1000 + i * 100 + j, i + 2);
        nodes.push(grandchild);
        branch.push(key(grandchild.identity));
        if (j % 3 === 0) {
          const descendant = processNode(
            10000 + i * 100 + j,
            grandchild.identity.pid,
          );
          nodes.push(descendant);
          branch.push(key(descendant.identity));
        }
      }
      branches.push(branch);
    }
    const layout = treeLayout(nodes);
    for (const p of layout.values())
      assert.equal(p.y, p.depth * 6.5, "tree depth determines the row");
    const bounds = branches.map((ids) => {
      const xs = ids.map((id) => layout.get(id)!.x);
      return { min: Math.min(...xs), max: Math.max(...xs) };
    });
    for (let i = 1; i < bounds.length; i++)
      assert.ok(
        bounds[i - 1].max < bounds[i].min,
        "sibling subtrees occupy disjoint horizontal ranges",
      );

    const forestNodes = Array.from({ length: 60 }, (_, i) => [
      processNode(i * 10 + 1),
      processNode(i * 10 + 2, i * 10 + 1),
      processNode(i * 10 + 3, i * 10 + 1),
      processNode(i * 10 + 4, i * 10 + 2),
    ]).flat();
    const forest = treeLayout(forestNodes);
    const roots = [...forest.values()].filter((p) => p.parent === null);
    assert.ok(
      new Set(roots.map((p) => p.y)).size > 1,
      "different root trees can wrap onto multiple rows",
    );
    for (const p of forest.values())
      if (p.parent)
        assert.equal(
          p.y - forest.get(p.parent)!.y,
          6.5,
          "each wrapped tree keeps its internal rows aligned by depth",
        );
  });

  test("process tree and stable layout", async () => {
    const assert: typeof import("node:assert/strict") = (
      await import("node:assert/strict")
    ).default;
    assert.deepEqual(treeLayout([]), new Map());
    const tree = treeLayout([
      processNode(1),
      processNode(2, 1),
      processNode(3, 1),
      processNode(4, 2),
      processNode(8, 99),
    ]);
    assert.equal(tree.get("1:10")!.y, 0);
    assert.ok(tree.get("2:20")!.y > tree.get("1:10")!.y);
    assert.ok(tree.get("4:40")!.y > tree.get("2:20")!.y);
    assert.equal(
      tree.get("1:10")!.x,
      (tree.get("2:20")!.x + tree.get("3:30")!.x) / 2,
    );
    assert.equal(tree.get("8:80")!.y, 0, "missing parent becomes a root");
    const cyclic = treeLayout([
      {
        identity: { pid: 10, start_time_ticks: 1 },
        parent_id: { pid: 11, start_time_ticks: 1 },
      },
      {
        identity: { pid: 11, start_time_ticks: 1 },
        parent_id: { pid: 10, start_time_ticks: 1 },
      },
    ]);
    assert.equal(cyclic.size, 2);
    assert.ok([...cyclic.values()].some((v) => v.parent === null));
    assert.deepEqual(
      [
        ...treeLayout([
          processNode(3, 1),
          processNode(1),
          processNode(2, 1),
        ]).entries(),
      ],
      [
        ...treeLayout([
          processNode(1),
          processNode(2, 1),
          processNode(3, 1),
        ]).entries(),
      ],
    );
    const fanout = treeLayout([
      processNode(1),
      ...Array.from({ length: 400 }, (_, i) => processNode(i + 2, 1)),
    ]);
    const fanoutX = [...fanout.values()].map((v) => v.x);
    assert.ok(
      Math.max(...fanoutX) - Math.min(...fanoutX) > 400,
      "large sibling groups expand horizontally",
    );
    assert.ok(
      [...fanout.values()].every((v) => v.y === v.depth * 6.5),
      "siblings stay on the same row even for large groups",
    );
    const originalNodes = [
      processNode(1),
      processNode(2, 1),
      processNode(3, 1),
    ];
    const anchored = stableLayout(originalNodes);
    assert.deepEqual(anchored, treeLayout(originalNodes));
    const coordinates = (layout: ReadonlyMap<string, TreePosition>) =>
      [...layout].map(([id, p]) => [id, { x: p.x, y: p.y }]).sort();
    assert.deepEqual(
      coordinates(stableLayout([...originalNodes].reverse(), anchored)),
      coordinates(anchored),
    );
    const grownNodes = [
      ...originalNodes,
      processNode(4, 2),
      processNode(5, 1),
      processNode(6),
    ];
    const grown = stableLayout(grownNodes, anchored);
    for (const [id, p] of anchored)
      assert.deepEqual(grown.get(id), p, "existing nodes stay anchored");
    assert.deepEqual(
      coordinates(stableLayout([...grownNodes].reverse(), anchored)),
      coordinates(grown),
    );
    const changed = stableLayout(
      [processNode(2, 99), processNode(3, 2), processNode(4, 2)],
      grown,
    );
    for (const [id, p] of changed)
      assert.deepEqual(
        { x: p.x, y: p.y },
        { x: grown.get(id)!.x, y: grown.get(id)!.y },
      );
    assert.equal(changed.has("1:10"), false, "removed identities are released");
    const reused = stableLayout(
      [
        processNode(1),
        processNode(2, 1),
        {
          identity: { pid: 3, start_time_ticks: 999 },
          parent_id: processNode(1).identity,
        },
      ],
      anchored,
    );
    assert.equal(reused.has("3:30"), false);
    assert.ok(reused.has("3:999"), "PID reuse is a new identity");
    const vacant = stableLayout(
      originalNodes.filter((n) => n.identity.pid !== 5),
      grown,
    );
    const refilled = stableLayout(
      [...originalNodes, processNode(7, 1)],
      vacant,
    );
    assert.deepEqual(
      { x: refilled.get("7:70")!.x, y: refilled.get("7:70")!.y },
      { x: grown.get("5:50")!.x, y: grown.get("5:50")!.y },
      "vacated child position is reusable",
    );
    const large = stableLayout(
      [
        processNode(1),
        ...Array.from({ length: 1000 }, (_, i) => processNode(i + 2, 1)),
      ],
      stableLayout([processNode(1)]),
    );
    const places = [...large.values()];
    for (let i = 0; i < places.length; i++)
      for (let j = i + 1; j < places.length; j++) {
        assert.ok(
          Math.abs(places[i].x - places[j].x) >= 4.8 - 1e-9 ||
            Math.abs(places[i].y - places[j].y) >= 6.5 - 1e-9,
          "new placements do not overlap",
        );
      }
    assert.deepEqual(stableLayout([], grown), new Map());
    const stableCycle = stableLayout(
      [processNode(1, 2), processNode(2, 1), processNode(9)],
      anchored,
    );
    assert.equal(stableCycle.size, 3);
    assert.ok(
      [...stableCycle.values()].every(
        (p) => Number.isFinite(p.x) && Number.isFinite(p.y),
      ),
    );
  });
}
