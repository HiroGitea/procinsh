// @vitest-environment jsdom
import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import * as T from "three";
import {
  ALL_HOSTS,
  HOST_LAYER_GAP,
  composeHosts,
  hostLayers,
  qualifyActivity,
  qualifySnapshot,
} from "../src/space/hosts.js";
import {
  hostUrl,
  mountHostControls,
  readHostParam,
} from "../src/space/host-controls.js";
import { SpaceDataStore } from "../src/space/data.js";
import { createSpaceScene } from "../src/space/scene.js";
import { createSpaceDetails } from "../src/space/details.js";
import { spaceElement } from "../src/space/dom-types.js";
import { key } from "../src/space/model.js";
import {
  fdEndpoint,
  fdRelation,
  processInfo,
  rdmaEndpoint,
  rdmaRelation,
  socketEndpoint,
  spaceActivity,
} from "./support/fixtures.js";
import { TestEventSource } from "./support/mocks.js";
import type {
  CrossHostLinks,
  HostsResponse,
  HostSummary,
  SystemSnapshot,
} from "../src/shared/api-types.js";

const pa = { pid: 10, start_time_ticks: 1 },
  pb = { pid: 10, start_time_ticks: 1 }; // the same identity on another host
const socket = (id: string, process = pa, inode = "7") =>
  fdRelation({
    id,
    endpoint: fdEndpoint({
      process_id: process,
      kind: "socket",
      resource: { kind: "socket", device: { major: 0, minor: 8 }, inode },
    }),
    socket: socketEndpoint({
      network_peer: true,
      local: { ip: "10.0.0.1", port: 5000 },
      remote: { ip: "10.0.0.2", port: 443 },
    }),
    label: "TCP ESTABLISHED 10.0.0.2:443",
  });
const qp = (qpn: number, remote: number, process = pa) =>
  rdmaRelation({
    endpoint: rdmaEndpoint({
      qpn,
      owner: { kind: "process", process_id: process },
    }),
    remote_qpn: remote,
  });
const host = (name: string, local = false): HostSummary => ({
  name,
  local,
  status: { state: "online", since: 0 },
  machine_id: name,
  version: "0",
  kernel: null,
  captured_at: null,
});
const snapshotA: SystemSnapshot = {
  processes: [processInfo({ identity: pa, name: "client" })],
  fd_relations: [socket("tcp")],
  rdma_relations: [qp(10, 20)],
  rdma_ports: [],
};
const snapshotB: SystemSnapshot = {
  processes: [
    processInfo({
      identity: pb,
      parent_id: null,
      name: "server",
    }),
  ],
  fd_relations: [socket("tcp", pb, "9")],
  rdma_relations: [qp(20, 10, pb)],
  rdma_ports: [],
};
const links: CrossHostLinks = {
  sockets: [
    { a: { host: "a", relation: "tcp" }, b: { host: "b", relation: "tcp" } },
  ],
  rdma: [
    {
      a: { host: "a", relation: "rdma:ib0/1:10" },
      b: { host: "b", relation: "rdma:ib0/1:20" },
      evidence: { gid: true, cm: false },
    },
  ],
};

describe("multi-host composition", () => {
  test("qualification tags every identity and keeps hosts apart", () => {
    const a = qualifySnapshot("a", snapshotA);
    expect(a.processes[0].identity).toEqual({ ...pa, host: "a" });
    expect(a.fd_relations[0].id).toBe("a@tcp");
    expect(a.fd_relations[0].endpoint.process_id.host).toBe("a");
    expect(a.rdma_relations![0].endpoint.host).toBe("a");
    const owner = a.rdma_relations![0].endpoint.owner;
    expect(owner.kind === "process" && owner.process_id.host).toBe("a");
    expect(key(a.processes[0].identity)).not.toBe(
      key(qualifySnapshot("b", snapshotB).processes[0].identity),
    );
    const activity = qualifyActivity(
      "a",
      spaceActivity({
        ipc: [
          {
            process_id: pa,
            resource: snapshotA.fd_relations[0].endpoint.resource,
            write: true,
            bytes: 1,
            count: 1,
          },
        ],
        signals: [
          {
            timestamp_ns: 0,
            src_pid: 10,
            dst_pid: 10,
            signal: 15,
            source_id: pa,
            destination_id: pa,
          },
        ],
      }),
    );
    expect(activity.ipc![0].process_id.host).toBe("a");
    expect(activity.signals![0].destination_id.host).toBe("a");
  });

  test("cross-host links replace both external ends", () => {
    const composed = composeHosts(
      new Map([
        ["a", qualifySnapshot("a", snapshotA)],
        ["b", qualifySnapshot("b", snapshotB)],
      ]),
      links,
    );
    expect(composed.processes).toHaveLength(2);
    expect(composed.fd_relations.map((r) => r.id)).toEqual(["a@tcp|b@tcp"]);
    const [tcp] = composed.fd_relations;
    expect(tcp.peer?.process_id.host).toBe("b");
    expect(tcp.label).toBe("TCP ESTABLISHED 10.0.0.2:443 ↔ b");
    expect(composed.rdma_relations!.map((r) => r.peer?.host)).toEqual(["b"]);
    // Links to relations not received yet keep the external ends.
    const partial = composeHosts(
      new Map([["a", qualifySnapshot("a", snapshotA)]]),
      links,
    );
    expect(partial.fd_relations.map((r) => r.id)).toEqual(["a@tcp"]);
    expect([...hostLayers(["a", "b"], partial)]).toEqual([
      ["a", 0],
      ["b", -HOST_LAYER_GAP],
    ]);
    expect([...hostLayers([], composed).keys()]).toEqual(["a", "b"]);
  });

  test("the merged stream stacks hosts and routes their activity", () => {
    let source: TestEventSource | undefined;
    const data = new SpaceDataStore(undefined, undefined, (url) => {
      source = new TestEventSource(url);
      return source.asEventSource();
    });
    data.host = ALL_HOSTS;
    data.start();
    expect(source!.url).toBe("/api/system/events?host=all");
    const hosts: HostsResponse = {
      local: "a",
      hosts: [host("a", true), host("b")],
    };
    source!.emit("hosts", hosts);
    source!.emit("snapshot", {
      ...snapshotA,
      host: "a",
      kind: "full",
      sequence: 1,
    });
    source!.emit("snapshot", {
      ...snapshotB,
      host: "b",
      kind: "full",
      sequence: 4,
    });
    source!.emit("links", links);
    const a = data.nodes.get(key({ ...pa, host: "a" }))!,
      b = data.nodes.get(key({ ...pb, host: "b" }))!;
    expect([a.pos.z, b.pos.z]).toEqual([0, -HOST_LAYER_GAP]);
    expect(data.hostLayerZ.get("b")).toBe(-HOST_LAYER_GAP);
    expect(data.snapshot.fd_relations.map((r) => r.id)).toEqual([
      "a@tcp|b@tcp",
    ]);
    expect(data.network.size).toBe(0);
    expect(data.rdmaLinks.map((l) => [l.from, l.to])).toEqual([
      [key(a.identity), key(b.identity)],
    ]);
    // Deltas apply to each host's own baseline and sequence.
    source!.emit("snapshot", {
      kind: "delta",
      host: "b",
      sequence: 5,
      base_sequence: 4,
      processes: [
        ...snapshotB.processes,
        processInfo({
          identity: { pid: 11, start_time_ticks: 2 },
          name: "worker",
        }),
      ],
      fd_relations_delta: { upsert: [], remove: [] },
    });
    expect(data.nodes.get("b@11:2")?.pos.z).toBe(-HOST_LAYER_GAP);
    expect(data.nodes.has("a@10:1")).toBe(true);
    const update = data.ingestActivity(
      qualifyActivity(
        "b",
        spaceActivity({
          ipc: [
            {
              process_id: pb,
              resource: snapshotB.fd_relations[0].endpoint.resource,
              write: true,
              bytes: 8,
              count: 1,
            },
          ],
        }),
      ),
    );
    expect(update.routes).toEqual([
      expect.objectContaining({
        kind: "connection",
        id: "a@tcp|b@tcp",
        direction: -1,
      }),
    ]);
    source!.emit("host", { host: "b", status: { state: "offline", since: 1 } });
    expect(data.hosts[1].status.state).toBe("offline");
    data.stop();
  });

  test("scene floors each host and joins layers", () => {
    const data = new SpaceDataStore();
    data.hosts = [host("a", true), host("b")];
    data.replaceSnapshot(
      composeHosts(
        new Map([
          ["a", qualifySnapshot("a", snapshotA)],
          ["b", qualifySnapshot("b", snapshotB)],
        ]),
        links,
      ),
      true,
    );
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
        hostLabels: new Map([["b", "b · offline"]]),
      },
      null,
    );
    const view = scene.renderView();
    expect(view.layerViews!.map((l) => [l.host, l.label])).toEqual([
      ["a", "a"],
      ["b", "b · offline"],
    ]);
    expect(view.layerViews![1].pos.z).toBeLessThan(-HOST_LAYER_GAP);
    const tcp = view.edgeViews.find((e) => e.e.id === "a@tcp|b@tcp")!;
    expect(tcp.curve.getPoint(0).z).toBeGreaterThan(0);
    expect(tcp.curve.getPoint(1).z).toBeLessThan(-HOST_LAYER_GAP + 8.2);
    const [rdma] = view.rdmaLinkViews;
    // From the base of the upper host's process to the roof of the lower one.
    expect(rdma.curve.getPoint(0).z).toBeCloseTo(0);
    expect(rdma.curve.getPoint(1).z).toBeCloseTo(-HOST_LAYER_GAP + 8);
    expect(
      scene.pickingView().objects.some((o) => o.name === "host-layers"),
    ).toBe(false);
    expect(scene.root.getObjectByName("host-layers")).toBeInstanceOf(
      T.InstancedMesh,
    );
    scene.dispose();
  });
});

describe("host selection", () => {
  test("URL parameter round trip", () => {
    expect(readHostParam("http://x/space?host=node%20b")).toBe("node b");
    expect(readHostParam("http://x/space?tour=1")).toBeNull();
    expect(hostUrl("all", "http://x/space?tour=1")).toBe(
      "http://x/space?tour=1&host=all",
    );
    expect(hostUrl(null, "http://x/space?host=b&tour=1")).toBe(
      "http://x/space?tour=1",
    );
  });

  test("the selector appears for several hosts and reports choices", async () => {
    document.body.innerHTML = '<nav class="tools"></nav>';
    let hosts: HostsResponse = { local: "a", hosts: [host("a", true)] };
    let selected: string | null = null;
    const changes: (string | null)[] = [];
    const controls = mountHostControls(
      document.querySelector(".tools")!,
      () => selected,
      (value) => changes.push(value),
      async () => hosts,
      60_000,
    );
    await controls.refresh();
    const wrapper = document.getElementById("host-controls")!,
      select = document.getElementById("host") as HTMLSelectElement;
    expect(wrapper.hidden).toBe(true);
    hosts = {
      local: "a",
      hosts: [
        host("a", true),
        { ...host("b"), status: { state: "offline", since: 0 } },
      ],
    };
    await controls.refresh();
    expect(wrapper.hidden).toBe(false);
    expect([...select.options].map((o) => [o.value, o.textContent])).toEqual([
      ["", "a (local)"],
      ["b", "b (offline)"],
      ["all", "All hosts (layers)"],
    ]);
    select.value = "all";
    select.dispatchEvent(new Event("change"));
    select.value = "";
    select.dispatchEvent(new Event("change"));
    expect(changes).toEqual(["all", null]);
    selected = "gone";
    await controls.refresh();
    expect(select.value).toBe("gone");
    expect(select.selectedOptions[0].textContent).toBe("gone (unavailable)");
    controls.dispose();
    expect(document.getElementById("host-controls")).toBeNull();
  });

  test("remote processes have no process page link", () => {
    document.body.innerHTML = readFileSync(
      resolve(import.meta.dirname, "../space/index.html"),
      "utf8",
    );
    const data = new SpaceDataStore();
    data.replaceSnapshot(qualifySnapshot("b", snapshotB));
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
    const selection = {
      process: "b@10:1",
      connection: null,
      network: null,
      file: null,
      netlink: null,
      rdma: null,
    };
    details.update({ ...input, linkable: () => false }, selection);
    expect(spaceElement("name").hasAttribute("href")).toBe(false);
    expect(spaceElement("pid").textContent).toContain("b · PID 10");
    details.update({ ...input, linkable: () => true }, selection);
    expect(spaceElement("name").getAttribute("href")).toContain("/process/10");
  });
});
