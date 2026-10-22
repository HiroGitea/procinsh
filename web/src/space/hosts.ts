// Pure multi-host helpers: qualify host-local identities and compose one snapshot.
import type {
  CrossHostLinks,
  FdRelation,
  ProcessId,
  RdmaEndpoint,
  RdmaRelation,
  SpaceActivity,
  SystemSnapshot,
} from "../shared/api-types.js";

/** The `?host=` value of the merged view. */
export const ALL_HOSTS = "all";
/** Vertical distance between host layers; one host's markers and files fit inside. */
export const HOST_LAYER_GAP = 48;

const id = (host: string, value: ProcessId): ProcessId => ({ ...value, host });
const relationId = (host: string, value: string) => `${host}@${value}`;

function rdmaEndpoint(host: string, endpoint: RdmaEndpoint): RdmaEndpoint {
  return {
    ...endpoint,
    host,
    owner:
      endpoint.owner.kind === "process"
        ? { ...endpoint.owner, process_id: id(host, endpoint.owner.process_id) }
        : endpoint.owner,
  };
}

/** Adds `host` to every process identity and prefixes relation IDs. */
export function qualifySnapshot(
  host: string,
  snapshot: SystemSnapshot,
): SystemSnapshot {
  return {
    ...snapshot,
    processes: snapshot.processes.map((p) => ({
      ...p,
      identity: id(host, p.identity),
      parent_id: p.parent_id && id(host, p.parent_id),
    })),
    fd_relations: snapshot.fd_relations.map((r) => ({
      ...r,
      id: relationId(host, r.id),
      endpoint: { ...r.endpoint, process_id: id(host, r.endpoint.process_id) },
      peer: r.peer && { ...r.peer, process_id: id(host, r.peer.process_id) },
    })),
    rdma_relations: (snapshot.rdma_relations ?? []).map((r) => ({
      ...r,
      id: relationId(host, r.id),
      endpoint: rdmaEndpoint(host, r.endpoint),
      peer: r.peer && rdmaEndpoint(host, r.peer),
      cm: r.cm && {
        ...r.cm,
        owner:
          r.cm.owner.kind === "process"
            ? { ...r.cm.owner, process_id: id(host, r.cm.owner.process_id) }
            : r.cm.owner,
      },
    })),
    rdma_ports: (snapshot.rdma_ports ?? []).map((p) => ({ ...p, host })),
  };
}

export function qualifyActivity(
  host: string,
  activity: SpaceActivity,
): SpaceActivity {
  return {
    ...activity,
    cpu: activity.cpu?.map((e) => ({
      ...e,
      process_id: id(host, e.process_id),
    })),
    ipc: activity.ipc?.map((e) => ({
      ...e,
      process_id: id(host, e.process_id),
    })),
    files: activity.files?.map((e) => ({
      ...e,
      process_id: id(host, e.process_id),
    })),
    signals: activity.signals?.map((e) => ({
      ...e,
      source_id: id(host, e.source_id),
      destination_id: id(host, e.destination_id),
    })),
  };
}

/**
 * Concatenates qualified host snapshots in layer order. Each cross-host link
 * replaces its two external relations with one relation whose peer is on the
 * other host; links to relations not yet received are skipped.
 */
export function composeHosts(
  snapshots: ReadonlyMap<string, SystemSnapshot>,
  links: CrossHostLinks,
): SystemSnapshot {
  const all = [...snapshots.values()];
  const fd = new Map<string, FdRelation>(
    all.flatMap((s) => s.fd_relations.map((r) => [r.id, r] as const)),
  );
  const rdma = new Map<string, RdmaRelation>(
    all.flatMap((s) => (s.rdma_relations ?? []).map((r) => [r.id, r] as const)),
  );
  const joined: { fd: FdRelation[]; rdma: RdmaRelation[] } = {
    fd: [],
    rdma: [],
  };
  for (const link of links.sockets) {
    const [a, b] = [link.a, link.b].map((end) =>
      fd.get(relationId(end.host, end.relation)),
    );
    if (!a || !b) continue;
    fd.delete(a.id);
    fd.delete(b.id);
    joined.fd.push({
      ...a,
      id: `${a.id}|${b.id}`,
      peer: b.endpoint,
      label: `${a.label} ↔ ${link.b.host}`,
      candidate: false,
      shared: false,
    });
  }
  for (const link of links.rdma) {
    const [a, b] = [link.a, link.b].map((end) =>
      rdma.get(relationId(end.host, end.relation)),
    );
    if (!a || !b) continue;
    rdma.delete(a.id);
    rdma.delete(b.id);
    joined.rdma.push({
      ...a,
      id: `${a.id}|${b.id}`,
      peer: b.endpoint,
      label: `RDMA ${link.a.host} QP ${a.endpoint.qpn} ↔ ${link.b.host} QP ${b.endpoint.qpn}`,
    });
  }
  return {
    processes: all.flatMap((s) => s.processes),
    fd_relations: [...fd.values(), ...joined.fd],
    rdma_relations: [...rdma.values(), ...joined.rdma],
    rdma_ports: all.flatMap((s) => s.rdma_ports ?? []),
  };
}

/** Host layers top to bottom: the given order, then hosts first seen in `snapshot`. */
export function hostLayers(
  order: readonly string[],
  snapshot: Pick<SystemSnapshot, "processes">,
) {
  const hosts = [...order];
  for (const p of snapshot.processes) {
    const host = p.identity.host;
    if (host !== undefined && !hosts.includes(host)) hosts.push(host);
  }
  return new Map(
    hosts.map((host, index) => [host, 0 - index * HOST_LAYER_GAP]),
  );
}
