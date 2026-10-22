// @vitest-environment jsdom
import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import * as T from "three";
import {
  isRdmaRelationVisible,
  rdmaDeviceId,
  rdmaDeviceLayout,
  rdmaModel,
} from "../src/space/rdma.js";
import { SpaceDataStore } from "../src/space/data.js";
import { createSpaceScene } from "../src/space/scene.js";
import { createSpaceDetails } from "../src/space/details.js";
import { spaceElement } from "../src/space/dom-types.js";
import { key } from "../src/space/model.js";
import {
  fdEndpoint,
  fdRelation,
  processInfo,
  socketEndpoint,
  rdmaEndpoint,
  rdmaPort,
  rdmaRelation,
  emptyRdmaInput,
} from "./support/fixtures.js";
import { TestEventSource } from "./support/mocks.js";
import type { RdmaRelation, SystemSnapshot } from "../src/shared/api-types.js";
import type { SelectionState } from "../src/space/types.js";

const process = { pid: 42, start_time_ticks: 7 };
const REMOTE_GID = "fe80:0000:0000:0000:f452:1403:0080:de51";
const nfs = rdmaRelation({
  cm: {
    id: 0,
    owner: { kind: "kernel", name: "rpcrdma" },
    state: { kind: "connect" },
    port_space: { kind: "tcp" },
    source: { kind: "inet", ip: "10.10.10.2", port: 58985 },
    destination: { kind: "inet", ip: "10.10.10.1", port: 20049 },
  },
  remote_gid: REMOTE_GID,
  label: "RDMA RC QP 524 → QP 650 10.10.10.1:20049",
});
const management = rdmaRelation({
  endpoint: rdmaEndpoint({
    qpn: 1,
    qp_type: { kind: "gsi" },
    owner: { kind: "kernel", name: "ib_core" },
  }),
  remote_qpn: null,
});
const user = (qpn: number, extra: Partial<RdmaRelation> = {}) =>
  rdmaRelation({
    endpoint: rdmaEndpoint({
      qpn,
      owner: { kind: "process", process_id: process },
    }),
    remote_qpn: qpn + 100,
    remote_gid: REMOTE_GID,
    ...extra,
  });
const loopback = rdmaRelation({
  id: "rdma:ib0/1:10/rdma:ib0/1:20",
  endpoint: rdmaEndpoint({
    qpn: 10,
    owner: { kind: "process", process_id: process },
  }),
  peer: rdmaEndpoint({ qpn: 20, owner: { kind: "unknown", pid: 9 } }),
  remote_qpn: 20,
  label: "RDMA RC QP 10 ↔ QP 20",
});
const snapshot = (rdma: RdmaRelation[]): SystemSnapshot => ({
  processes: [processInfo({ identity: process, name: "mpi" })],
  fd_relations: [],
  rdma_relations: rdma,
  rdma_ports: [rdmaPort(), rdmaPort({ port: 2, state: "DOWN", lid: null })],
});

describe("RDMA SPACE model", () => {
  test("groups unpaired QPs by anchor and remote, links local pairs", () => {
    const live = new Set([key(process)]);
    const { groups, links, devices } = rdmaModel(
      [nfs, management, user(30), user(31), loopback],
      [rdmaPort(), rdmaPort({ device: "mlx5_0" })],
      live,
    );
    expect(isRdmaRelationVisible(management)).toBe(false);
    expect([...devices.values()].map((d) => d.device)).toEqual([
      "ib0",
      "mlx5_0",
    ]);
    expect(devices.get(rdmaDeviceId("ib0"))!.ports.map((p) => p.port)).toEqual([
      1,
    ]);
    const labels = [...groups.values()].map((g) => [g.anchor, g.label]);
    expect(labels).toEqual([
      [rdmaDeviceId("ib0"), "rpcrdma · RDMA RC 10.10.10.1:20049 ×1"],
      [key(process), `RDMA RC ${REMOTE_GID} ×2`],
    ]);
    expect(links).toEqual([
      {
        id: loopback.id,
        relation: loopback,
        from: key(process),
        to: rdmaDeviceId("ib0"),
      },
    ]);
    // A process outside the snapshot anchors its QPs to the device.
    const orphan = rdmaModel([user(30)], [], new Set());
    expect([...orphan.groups.values()][0].anchor).toBe(rdmaDeviceId("ib0"));
    expect([...orphan.devices.keys()]).toEqual([rdmaDeviceId("ib0")]);
    // Hosts of the merged view keep same-named devices apart.
    const merged = rdmaModel(
      [],
      [rdmaPort({ host: "a" }), rdmaPort({ host: "b" })],
      new Set(),
    );
    expect([...merged.devices.keys()]).toEqual([
      rdmaDeviceId("ib0", "a"),
      rdmaDeviceId("ib0", "b"),
    ]);
  });

  test("device anchors sit right of processes and keep their positions", () => {
    const first = rdmaDeviceLayout(
      [rdmaDeviceId("ib0"), rdmaDeviceId("mlx5_0")],
      [
        { x: -3, y: -6 },
        { x: 10, y: 4 },
      ],
    );
    expect(first.get(rdmaDeviceId("ib0"))).toEqual({ x: 18, y: -6, z: 0 });
    expect(first.get(rdmaDeviceId("mlx5_0"))).toEqual({ x: 18, y: 0, z: 0 });
    const moved = rdmaDeviceLayout(
      ["ib0", "mlx5_0", "rxe0"].map((d) => rdmaDeviceId(d)),
      [{ x: 30, y: 0 }],
      first,
    );
    expect(moved.get(rdmaDeviceId("ib0"))).toEqual(
      first.get(rdmaDeviceId("ib0")),
    );
    expect(moved.get(rdmaDeviceId("rxe0"))).toEqual({ x: 39, y: 0, z: 0 });
    expect(rdmaDeviceLayout([], []).size).toBe(0);
  });

  test("SSE deltas update RDMA relations and ports; markers avoid network cells", () => {
    let source: TestEventSource | undefined;
    const data = new SpaceDataStore(undefined, undefined, (url) => {
      source = new TestEventSource(url);
      return source.asEventSource();
    });
    data.start();
    source!.emit("snapshot", {
      ...snapshot([nfs, user(30)]),
      kind: "full",
      sequence: 1,
    });
    expect(data.rdma.size).toBe(2);
    expect(data.rdmaDevices.has(rdmaDeviceId("ib0"))).toBe(true);
    source!.emit("snapshot", {
      kind: "delta",
      sequence: 2,
      base_sequence: 1,
      processes: snapshot([]).processes,
      fd_relations: [],
      rdma_ports: [rdmaPort()],
      rdma_relations_delta: { upsert: [user(31)], remove: [nfs.id] },
    });
    expect(data.snapshot.rdma_relations!.map((r) => r.id)).toEqual([
      user(30).id,
      user(31).id,
    ]);
    expect(data.snapshot.rdma_ports).toHaveLength(1);
    expect("rdma_relations_delta" in data.snapshot).toBe(false);
    const [group] = data.rdma.values();
    expect(group.members).toHaveLength(2);
    // Network and RDMA markers above one process share a lattice without overlap.
    data.replaceSnapshot({
      ...data.snapshot,
      fd_relations: [
        fdRelation({
          id: "tcp",
          endpoint: fdEndpoint({
            process_id: process,
            kind: "socket",
            resource: {
              kind: "socket",
              device: { major: 0, minor: 8 },
              inode: "9",
            },
          }),
          socket: socketEndpoint({
            network_peer: true,
            remote: { ip: "192.0.2.1", port: 443 },
          }),
        }),
      ],
    });
    const cells = [
      ...data.networkPositions.values(),
      ...data.rdmaPositions.values(),
    ].map((p) => `${p.x}:${p.y}:${p.z}`);
    expect(data.networkPositions.size).toBe(1);
    expect(new Set(cells).size).toBe(2);
    // An older server without RDMA fields keeps an empty RDMA view.
    source!.emit("snapshot", {
      kind: "full",
      sequence: 3,
      processes: snapshot([]).processes,
      fd_relations: [],
    });
    expect(data.rdma.size).toBe(0);
    expect(data.snapshot.rdma_relations).toEqual([]);
    data.stop();
  });

  test("scene draws devices, markers and links that pick to RDMA IDs", () => {
    const data = new SpaceDataStore();
    data.replaceSnapshot(snapshot([nfs, user(30), loopback, management]));
    const scene = createSpaceScene();
    scene.rebuild(
      {
        snapshot: data.displaySnapshot,
        nodes: data.nodes,
        network: data.network,
        networkPositions: data.networkPositions,
        files: new Map(),
        netlink: new Map(),
        filePositions: new Map(),
        netlinkPositions: new Map(),
        visible: new Set(data.nodes.keys()),
        rdma: data.rdma,
        rdmaLinks: data.rdmaLinks,
        rdmaDevices: data.rdmaDevices,
        rdmaPositions: data.rdmaPositions,
        rdmaDevicePositions: data.rdmaDevicePositions,
      },
      null,
    );
    const view = scene.renderView();
    expect(view.rdmaDeviceViews.map((v) => v.device.label)).toEqual(["ib0"]);
    expect(view.rdmaViews.map((v) => v.group.label).sort()).toEqual([
      `RDMA RC ${REMOTE_GID} ×1`,
      "rpcrdma · RDMA RC 10.10.10.1:20049 ×1",
    ]);
    expect(view.rdmaLinkViews.map((v) => v.link.id)).toEqual([loopback.id]);
    const markers = scene.root.getObjectByName("rdma-markers")!;
    expect(markers.userData.rdma).toEqual(
      view.rdmaViews.map((v) => v.group.id),
    );
    const lines = scene.root.getObjectByName("rdma-lines") as T.LineSegments;
    expect(new Set(lines.userData.rdmaPaths)).toEqual(
      new Set([...view.rdmaViews.map((v) => v.group.id), loopback.id]),
    );
    // Markers start above their anchors: the process roof or the device plate.
    for (const v of view.rdmaViews) {
      const start = v.curve.getPoint(0);
      expect(start.z).toBeCloseTo(v.group.device ? 0.5 : 8);
    }
    const selection: SelectionState = {
      process: null,
      connection: null,
      network: null,
      file: null,
      netlink: null,
      rdma: loopback.id,
    };
    scene.updateSelection(selection);
    expect(scene.root.getObjectByName("selected-connection")!.visible).toBe(
      true,
    );
    // Process search hides process-anchored QPs, while device anchors remain.
    scene.rebuild(
      {
        snapshot: data.displaySnapshot,
        nodes: data.nodes,
        network: data.network,
        networkPositions: data.networkPositions,
        files: new Map(),
        netlink: new Map(),
        filePositions: new Map(),
        netlinkPositions: new Map(),
        visible: new Set(),
        rdma: data.rdma,
        rdmaLinks: data.rdmaLinks,
        rdmaDevices: data.rdmaDevices,
        rdmaPositions: data.rdmaPositions,
        rdmaDevicePositions: data.rdmaDevicePositions,
      },
      null,
    );
    expect(scene.renderView().rdmaViews.map((v) => v.group.device)).toEqual([
      "ib0",
    ]);
    expect(scene.renderView().rdmaLinkViews).toEqual([]);
    scene.dispose();
  });

  test("details show QP addresses, local ports and device ports", () => {
    document.body.innerHTML = readFileSync(
      resolve(import.meta.dirname, "../space/index.html"),
      "utf8",
    );
    const data = new SpaceDataStore();
    data.replaceSnapshot(snapshot([nfs, loopback]));
    const details = createSpaceDetails(
      { connection() {}, network() {} },
      spaceElement,
    );
    const input = {
      nodes: data.nodes,
      snapshot: data.displaySnapshot,
      network: data.network,
      edgeStats: new Map(),
      files: new Map(),
      netlink: new Map(),
      rdma: data.rdma,
      rdmaLinks: data.rdmaLinks,
      rdmaDevices: data.rdmaDevices,
    };
    const select = (rdma: string): SelectionState => ({
      process: null,
      connection: null,
      network: null,
      file: null,
      netlink: null,
      rdma,
    });
    const [group] = data.rdma.values();
    details.update(input, select(group.id));
    const text = spaceElement("connection-endpoints").textContent;
    expect(spaceElement("connection-label").textContent).toBe(group.label);
    expect(spaceElement("connection-state").textContent).toBe(
      "RDMA queue pairs to a remote destination",
    );
    expect(spaceElement("connection-facts").textContent).toContain(
      "bypass the kernel",
    );
    expect(text).toContain("Kernel · rdma_cm");
    expect(text).toContain("QP · ib0/1 QP 524 · RC · RTS");
    expect(text).toContain(
      "Local LID 2 · GID fe80:0000:0000:0000:0002:c903:0028:45c7",
    );
    expect(text).toContain(`Remote QP 650 · GID ${REMOTE_GID}`);
    expect(text).toContain(
      "CM CONNECT · TCP · rpcrdma · 10.10.10.2:58985 → 10.10.10.1:20049",
    );
    details.update(input, select(loopback.id));
    expect(spaceElement("connection-state").textContent).toBe(
      "Connected queue pairs on this host",
    );
    expect(spaceElement("connection-endpoints").textContent).toContain(
      "PEER · ib0/1 QP 20",
    );
    expect(spaceElement("connection-endpoints").textContent).toContain("mpi");
    details.update(input, select(rdmaDeviceId("ib0")));
    expect(spaceElement("connection-state").textContent).toBe("RDMA device");
    expect(spaceElement("connection-endpoints").textContent).toContain(
      "Port 2 · DOWN",
    );
    expect(spaceElement("connection-endpoints").textContent).toContain(
      "InfiniBand · 40 Gb/sec (4X QDR)",
    );
    // Unknown IDs leave the panel untouched, like other stale selections.
    details.update({ ...input, ...emptyRdmaInput() }, select("missing"));
  });
});
