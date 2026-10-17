import { test } from "vitest";
import assert from "node:assert/strict";
import * as T from "three";
import { SocketPortAllocator } from "../src/space/socket-ports.js";
import { createSpaceScene } from "../src/space/scene.js";
import { socketOwnerKey } from "../src/space/model.js";
import type { SceneInput } from "../src/space/contracts.js";
import type { FdRelation } from "../src/shared/api-types.js";
import {
  processInfo,
  fdEndpoint,
  fdRelation,
  socketEndpoint,
} from "./support/fixtures.js";

function endpoint(pid: number, fd: number, inode = String(fd), minor = 1) {
  return fdEndpoint({
    process_id: { pid, start_time_ticks: 1 },
    fd,
    kind: "socket",
    resource: { kind: "socket", device: { major: 0, minor }, inode },
  });
}
function input(relations: FdRelation[]): SceneInput {
  const nodes = [0, 10, -10].map((x, i) => ({
    ...processInfo({ identity: { pid: i + 1, start_time_ticks: 1 } }),
    pos: { x, y: 0, z: 0 },
    regions: [],
  }));
  return {
    nodes: new Map(nodes.map((node) => [`${node.identity.pid}:1`, node])),
    visible: new Set(nodes.map((node) => `${node.identity.pid}:1`)),
    snapshot: { processes: nodes, fd_relations: relations },
    network: new Map(),
    networkPositions: new Map(),
    files: new Map(),
    filePositions: new Map(),
    netlink: new Map(),
    netlinkPositions: new Map(),
  };
}
function cylinderTips(scene: ReturnType<typeof createSpaceScene>) {
  const mesh = scene.root.getObjectByName("socket-port-stems");
  assert.ok(mesh instanceof T.InstancedMesh);
  const matrix = new T.Matrix4();
  return Array.from({ length: mesh.count }, (_, i) => {
    mesh.getMatrixAt(i, matrix);
    return new T.Vector3()
      .setFromMatrixPosition(matrix)
      .addScaledVector(new T.Vector3(0, 0, 1).transformDirection(matrix), 0.08);
  });
}

test("resource identities separate cylinders while FD aliases share one stable attachment", () => {
  const allocator = new SocketPortAllocator();
  const a = endpoint(1, 4, "socket-a"),
    b = endpoint(1, 16, "socket-b");
  const otherDevice = endpoint(1, 28, "socket-a", 2);
  const edges = [a, b, otherDevice, { ...a, fd: 100 }].map((endpoint, i) =>
    fdRelation({ id: String(i), endpoint }),
  );
  const before = allocator.allocate(input(edges));
  assert.equal(
    before.size,
    3,
    "identical FD slots and different devices cannot merge resources",
  );
  assert.equal(before.get(socketOwnerKey(a))!.connections.size, 2);
  assert.equal(
    new Set(
      [...before.values()].map((port) => port.position.toArray().join(":")),
    ).size,
    3,
  );
  const reordered = [...edges].reverse().map((edge) => ({
    ...edge,
    endpoint: { ...edge.endpoint, fd: edge.endpoint.fd + 1000 },
  }));
  const after = allocator.allocate(input(reordered));
  for (const [id, port] of before)
    assert.deepEqual(
      after.get(id)!.position.toArray(),
      port.position.toArray(),
      "FD renumbering and input ordering preserve socket positions",
    );
  allocator.allocate(input([edges[1]]));
  const added = allocator.allocate(
    input([edges[1], fdRelation({ endpoint: endpoint(1, 4, "new-socket") })]),
  );
  assert.deepEqual(
    added.get(socketOwnerKey(b))!.position.toArray(),
    before.get(socketOwnerKey(b))!.position.toArray(),
    "removing and adding other sockets does not move a surviving attachment",
  );
});

test("dense faces allocate distinct non-overlapping cylinders beyond twelve FD slots", () => {
  const edges = Array.from({ length: 100 }, (_, i) =>
    fdRelation({ id: String(i), endpoint: endpoint(1, i * 12) }),
  );
  const ports = [...new SocketPortAllocator().allocate(input(edges)).values()];
  assert.equal(ports.length, edges.length);
  for (let i = 0; i < ports.length; i++) {
    const port = ports[i];
    assert.ok(Math.abs(port.position.x - 1.26) < 1e-9);
    assert.ok(
      Math.abs(port.position.y) < 1.1 &&
        port.position.z > 3.3 &&
        port.position.z < 4.7,
    );
    for (const other of ports.slice(i + 1))
      assert.ok(
        port.position.distanceTo(other.position) >
          0.055 * (port.radiusScale + other.radiusScale),
        "different sockets have physically separate cylinders",
      );
  }
});

test("one socket serving peers on different sides uses one cylinder and perpendicular hull-clearing paths", () => {
  const source = endpoint(1, 4, "source");
  const edges = [2, 3].map((pid) =>
    fdRelation({
      id: `peer-${pid}`,
      endpoint: { ...source, fd: pid * 12 },
      peer: endpoint(pid, 8),
      candidate: true,
    }),
  );
  const data = input(edges),
    scene = createSpaceScene();
  try {
    scene.rebuild(data, null);
    const views = scene.renderView().edgeViews;
    assert.equal(cylinderTips(scene).length, 3);
    assert.deepEqual(
      views[0].curve.getPoint(0).toArray(),
      views[1].curve.getPoint(0).toArray(),
    );
    for (const { curve, e } of views) {
      assert.ok(curve.getTangent(0).dot(new T.Vector3(1, 0, 0)) > 0.999);
      const points = Array.from({ length: 33 }, (_, i) =>
        curve.getPoint(i / 32),
      );
      for (const pid of [1, e.peer!.process_id.pid]) {
        const pos = data.nodes.get(`${pid}:1`)!.pos;
        const hull = new T.Box3(
          new T.Vector3(pos.x - 1.1, -1.1, 0),
          new T.Vector3(pos.x + 1.1, 1.1, 8),
        );
        for (let i = 1; i < points.length; i++) {
          const delta = points[i].clone().sub(points[i - 1]);
          const hit = new T.Ray(
            points[i - 1],
            delta.clone().normalize(),
          ).intersectBox(hull, new T.Vector3());
          assert.ok(
            !hull.containsPoint(points[i - 1]) &&
              (!hit || hit.distanceTo(points[i - 1]) > delta.length() - 1e-7),
          );
        }
      }
    }
    scene.rebuild(
      input(
        [...edges].reverse().map((edge) => ({
          ...edge,
          endpoint: edge.peer!,
          peer: edge.endpoint,
        })),
      ),
      null,
    );
    for (const before of views) {
      const after = scene
        .renderView()
        .edgeViews.find((view) => view.e.id === before.e.id)!;
      for (const t of [0, 0.25, 0.5, 0.75, 1])
        assert.ok(
          before.curve.getPoint(t).distanceTo(after.curve.getPoint(1 - t)) <
            1e-8,
        );
    }
  } finally {
    scene.dispose();
  }
});

function networkInput() {
  const edges = [endpoint(1, 4), endpoint(1, 16), endpoint(1, 99, "4")].map(
    (endpoint, i) =>
      fdRelation({
        id: `network-${i}`,
        endpoint,
        socket: socketEndpoint({ network_peer: true }),
      }),
  );
  const data = input(edges);
  data.network = new Map([
    [
      "destination",
      {
        id: "destination",
        endpoint: edges[0].endpoint,
        socket: edges[0].socket!,
        members: [...edges],
        label: "remote",
      },
    ],
  ]);
  data.networkPositions = new Map([["destination", { x: 5, y: 5, z: 12 }]]);
  return data;
}

test("network groups retain per-socket cylinders and paths, and select every member path", () => {
  const data = networkInput(),
    scene = createSpaceScene();
  try {
    scene.rebuild(data, null);
    const views = scene.renderView().edgeViews,
      tips = cylinderTips(scene);
    assert.equal(
      tips.length,
      2,
      "two resources with an FD alias produce two cylinders",
    );
    assert.equal(
      scene.renderView().networkViews.length,
      1,
      "destination markers remain grouped",
    );
    assert.notDeepEqual(
      views[0].curve.getPoint(0).toArray(),
      views[1].curve.getPoint(0).toArray(),
    );
    assert.equal(
      views[0].curve,
      views[2].curve,
      "aliases use the same socket path",
    );
    for (const view of views) {
      assert.ok(
        tips.some((tip) => tip.distanceTo(view.curve.getPoint(0)) < 1e-6),
      );
      assert.ok(view.curve.getTangent(0).z > 0.999);
    }
    scene.updateSelection({
      process: null,
      connection: null,
      network: "destination",
      file: null,
      netlink: null,
    });
    const selected = scene.root.getObjectByName(
      "selected-connection",
    ) as T.LineSegments;
    assert.equal(
      selected.geometry.getAttribute("position").count,
      128,
      "group selection highlights both distinct socket paths",
    );
    scene.updateSelection({
      process: null,
      connection: "network-0",
      network: null,
      file: null,
      netlink: null,
    });
    assert.equal(selected.geometry.getAttribute("position").count, 64);
  } finally {
    scene.dispose();
  }
});

test("a socket observed in both network and process relations still has one cylinder", () => {
  const data = networkInput(),
    scene = createSpaceScene();
  const source = { ...data.snapshot.fd_relations[0].endpoint, fd: 300 };
  const local = fdRelation({
    id: "local",
    endpoint: source,
    peer: endpoint(2, 8, "peer"),
  });
  data.snapshot.fd_relations.push(local);
  try {
    scene.rebuild(data, null);
    assert.equal(cylinderTips(scene).length, 3);
    const views = scene.renderView().edgeViews;
    assert.deepEqual(
      views
        .find((v) => v.e.id === "local")!
        .curve.getPoint(0)
        .toArray(),
      views
        .find((v) => v.e.id === "network-0")!
        .curve.getPoint(0)
        .toArray(),
    );
  } finally {
    scene.dispose();
  }
});
