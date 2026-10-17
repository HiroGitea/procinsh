// Shared values passed between SPACE components; independent of their implementations.
import type * as T from "three";
import type {
  Process,
  ProcessId,
  FdEndpoint,
  SocketEndpoint,
  FdRelation,
  FileIdentity,
  CpuActivity,
  MemoryMap,
  SignalEvent,
} from "../shared/api-types.js";
export interface Position {
  x: number;
  y: number;
  z: number;
}
export interface TreePosition {
  x: number;
  y: number;
  depth: number;
  parent: string | null;
}
export interface NetworkGroup {
  id: string;
  endpoint: FdEndpoint;
  socket: SocketEndpoint;
  members: FdRelation[];
  label: string;
}
export interface NetlinkSocketInfo {
  id: string;
  endpoint: FdEndpoint;
  socket: SocketEndpoint;
  holders: FdEndpoint[];
  label: string;
}
export interface NetlinkSocketStats extends NetlinkSocketInfo {
  readBytes: number;
  writeBytes: number;
  readCount: number;
  writeCount: number;
}
export interface RecentFile {
  id: string;
  process_id: ProcessId;
  file: FileIdentity;
  path: string | null;
  readBytes: number;
  writeBytes: number;
  readCount: number;
  writeCount: number;
  label: string;
  last: number;
}
export type CpuGlow = CpuActivity & { last: number; window_ms: number };
export type Region = MemoryMap & { z: number; h: number };
/** A process with layout coordinates, independent of Three.js objects. */
export interface PlacedProcess extends Process {
  pos: Position;
  regions: Region[];
}
export interface EdgeStat {
  bytes: number;
  count: number;
  time: number;
}
export type ActivityRoute = {
  kind: "file" | "netlink" | "connection" | "port";
  socketOwnerKey?: string;
  id: string;
  direction?: number | null;
  count: number;
};
export interface ActivityUpdate {
  now: number;
  filesChanged: boolean;
  netlinkChanged: boolean;
  routes: ActivityRoute[];
  signals?: SignalEvent[];
}
/** Exact identities needed by animations, including staggered particles and pulses. */
export interface ActivityRetention {
  processes: Set<string>;
  connections: Set<string>;
  networks: Set<string>;
  files: Set<string>;
  netlink: Set<string>;
}
export function emptyActivityRetention(): ActivityRetention {
  return {
    processes: new Set(),
    connections: new Set(),
    networks: new Set(),
    files: new Set(),
    netlink: new Set(),
  };
}
export interface DataEvents {
  snapshot(): void;
  activity(update: ActivityUpdate): void;
  reset(): void;
  gap(): void;
  status(message: string | null): void;
}

export interface SelectionState {
  readonly process: string | null;
  readonly connection: string | null;
  readonly network: string | null;
  readonly file: string | null;
  readonly netlink: string | null;
}
export interface HoverState {
  readonly hoveredNetwork: string | null;
  readonly hoveredFile: string | null;
  readonly hoveredNetlink: string | null;
}
export type ConnectionPick = FdRelation & { networkId?: string };
export type PickResult =
  ConnectionPick | { fileId: string } | { netlinkId: string };
export interface RenderNode extends Process {
  pos: T.Vector3;
  regions: Region[];
}
export type ConnectionCurve =
  T.QuadraticBezierCurve3 | T.CubicBezierCurve3 | T.CurvePath<T.Vector3>;
export interface EdgeView {
  e: FdRelation;
  curve: ConnectionCurve;
  networkId?: string;
}
export interface NetworkView {
  group: NetworkGroup;
  pos: T.Vector3;
  curve: T.CubicBezierCurve3;
}
export interface FileView {
  file: RecentFile;
  pos: T.Vector3;
  curve: T.QuadraticBezierCurve3;
}

export interface NetlinkView {
  socket: NetlinkSocketStats;
  pos: T.Vector3;
  curve: T.QuadraticBezierCurve3;
}
