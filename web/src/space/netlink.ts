// Identify kernel-facing Netlink sockets and retain observed traffic per owner.
import { Display } from "../shared/display.js";
import type {
  FdEndpoint,
  FdRelation,
  IoActivity,
  SocketEndpoint,
} from "../shared/api-types.js";
import { ipcKey, key, socketOwnerKey } from "./model.js";
import type { RecentNetlinkSocket, NetlinkSocketInfo } from "./types.js";

/** USERSOCK retains ordinary socket rendering; other families are unaffected. */
export function isKernelNetlink(socket: SocketEndpoint | null | undefined) {
  const protocol = socket?.protocol;
  return (
    protocol?.kind === "other" &&
    protocol.family === 16 &&
    protocol.protocol !== 2
  );
}

const ownerKey = socketOwnerKey;

/** Shared ownership is not a communication peer. Each owner gets its own entry. */
function socketOwners(relations: readonly FdRelation[]) {
  const resources = new Map<
    string,
    { socket: SocketEndpoint; endpoints: Map<string, FdEndpoint> }
  >();
  for (const relation of relations) {
    if (!isKernelNetlink(relation.socket)) continue;
    const resource = ipcKey(relation.endpoint.resource);
    let group = resources.get(resource);
    if (!group) {
      group = { socket: relation.socket!, endpoints: new Map() };
      resources.set(resource, group);
    }
    for (const endpoint of [
      relation.endpoint,
      relation.shared ? relation.peer : null,
    ]) {
      if (!endpoint || ipcKey(endpoint.resource) !== resource) continue;
      group.endpoints.set(
        JSON.stringify([key(endpoint.process_id), endpoint.fd]),
        endpoint,
      );
    }
  }
  const owners = new Map<string, NetlinkSocketInfo>();
  for (const { socket, endpoints } of resources.values()) {
    const holders = [...endpoints.values()].sort(
      (a, b) =>
        a.process_id.pid - b.process_id.pid ||
        a.process_id.start_time_ticks - b.process_id.start_time_ticks ||
        a.fd - b.fd,
    );
    for (const endpoint of holders) {
      const id = ownerKey(endpoint);
      if (owners.has(id)) continue;
      const owned = holders.filter(
        (e) => key(e.process_id) === key(endpoint.process_id),
      );
      owners.set(id, {
        id,
        endpoint: {
          ...endpoint,
          fd_count: owned.reduce((n, e) => n + e.fd_count, 0),
        },
        socket,
        holders,
        label: `${Display.protocol(socket.protocol)} · FD ${endpoint.fd}`,
      });
    }
  }
  return owners;
}

/** Traffic survives socket closure for 30 seconds, but never process exit/reset. */
export class RecentNetlinkSockets {
  readonly entries = new Map<string, RecentNetlinkSocket>();
  private owners = new Map<string, NetlinkSocketInfo>();
  clear() {
    this.entries.clear();
    this.owners.clear();
  }
  sync(
    relations: readonly FdRelation[],
    now: number,
    live: ReadonlySet<string>,
    suppressed: ReadonlySet<string> = new Set(),
  ) {
    this.owners = socketOwners(relations);
    for (const [id, entry] of this.entries) {
      if (suppressed.has(id)) {
        this.entries.delete(id);
        continue;
      }
      const owner = this.owners.get(id);
      if (owner) Object.assign(entry, owner);
    }
    this.prune(now, live);
  }
  prune(now: number, live: ReadonlySet<string>) {
    let changed = false;
    for (const [id, entry] of this.entries) {
      if (
        now - entry.last >= 30000 ||
        !live.has(key(entry.endpoint.process_id))
      ) {
        this.entries.delete(id);
        changed = true;
      }
    }
    return changed;
  }
  /** Null means the current snapshot cannot classify this event as Netlink. */
  ingest(event: IoActivity, now: number, live: ReadonlySet<string>) {
    const owner = this.owners.get(ownerKey(event));
    if (!owner) return null;
    if (
      !live.has(key(event.process_id)) ||
      !(event.bytes > 0) ||
      !(event.count > 0)
    )
      return { id: owner.id, changed: false, observed: false };
    let entry = this.entries.get(owner.id);
    const changed = !entry;
    if (!entry) {
      const owned = [...this.entries.values()].filter(
        (e) => key(e.endpoint.process_id) === key(event.process_id),
      );
      const victim =
        owned.length >= 32
          ? owned[0]
          : this.entries.size >= 512
            ? this.entries.values().next().value
            : null;
      if (victim) this.entries.delete(victim.id);
      entry = {
        ...owner,
        last: now,
        readBytes: 0,
        writeBytes: 0,
        readCount: 0,
        writeCount: 0,
      };
    }
    entry[event.write ? "writeBytes" : "readBytes"] += event.bytes;
    entry[event.write ? "writeCount" : "readCount"] += event.count;
    entry.last = now;
    this.entries.delete(owner.id);
    this.entries.set(owner.id, entry);
    return { id: owner.id, changed, observed: true };
  }
}
