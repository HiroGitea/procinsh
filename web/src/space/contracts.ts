// Inputs and callbacks for SPACE geometry, picking, details, and rendering.
import type * as T from "three";
import type { ProcessId, SystemSnapshot } from "../shared/api-types.js";
import type {
  PlacedProcess,
  Position,
  NetworkGroup,
  RecentFile,
  NetlinkSocketStats,
  NetlinkView,
  EdgeStat,
  RenderNode,
  EdgeView,
  NetworkView,
  FileView,
  HoverState,
  RdmaDevice,
  RdmaDeviceView,
  RdmaGroup,
  RdmaLink,
  RdmaLinkView,
  RdmaView,
  HostLayerView,
} from "./types.js";

export interface SelectionActions {
  process(id: string | null, focus?: boolean): void;
  connection(id: string | null): void;
  network(id: string | null): void;
  file(id: string | null): void;
  netlink(id: string | null): void;
  rdma(id: string | null): void;
  clear(): void;
}
export interface SelectionTargets {
  processes: ReadonlySet<string>;
  connections: ReadonlySet<string>;
  networks: ReadonlySet<string>;
  files: ReadonlySet<string>;
  netlink: ReadonlySet<string>;
  rdma: ReadonlySet<string>;
}
export interface FileSceneInput {
  nodes: ReadonlyMap<string, PlacedProcess>;
  files: ReadonlyMap<string, RecentFile>;
  netlink: ReadonlyMap<string, NetlinkSocketStats>;
  filePositions: ReadonlyMap<string, Position>;
  netlinkPositions: ReadonlyMap<string, Position>;
  visible: ReadonlySet<string>;
}
/** RDMA groups, local QP links and device anchors with their positions. */
export interface RdmaInput {
  rdma: ReadonlyMap<string, RdmaGroup>;
  rdmaLinks: readonly RdmaLink[];
  rdmaDevices: ReadonlyMap<string, RdmaDevice>;
}
export interface SceneInput extends FileSceneInput, RdmaInput {
  snapshot: SystemSnapshot;
  network: ReadonlyMap<string, NetworkGroup>;
  networkPositions: ReadonlyMap<string, Position>;
  rdmaPositions: ReadonlyMap<string, Position>;
  rdmaDevicePositions: ReadonlyMap<string, Position>;
  /** Floor labels of the merged multi-host view, by host. */
  hostLabels?: ReadonlyMap<string, string>;
}
export interface WorldPositions {
  processes: Iterable<Position>;
  networks: Iterable<Position>;
  files: Iterable<Position>;
  netlink: Iterable<Position>;
}
export interface DetailsInput extends RdmaInput {
  nodes: ReadonlyMap<string, PlacedProcess>;
  /** Whether this server can show the process page; false for remote hosts. */
  linkable?: (id: ProcessId) => boolean;
  snapshot: SystemSnapshot;
  network: ReadonlyMap<string, NetworkGroup>;
  edgeStats: ReadonlyMap<string, EdgeStat>;
  files: ReadonlyMap<string, RecentFile>;
  netlink: ReadonlyMap<string, NetlinkSocketStats>;
}
/** Read afresh for each pointer event, since scene geometry is replaced on updates. */
export interface PickingView {
  hull: T.InstancedMesh | null;
  hullIds: readonly string[];
  nodes: ReadonlyMap<string, RenderNode>;
  objects: T.Object3D[];
}
export interface PickingInput extends RdmaInput {
  view: PickingView;
  edgeStats: ReadonlyMap<string, EdgeStat>;
  files: ReadonlyMap<string, RecentFile>;
  netlink: ReadonlyMap<string, NetlinkSocketStats>;
}
export interface SelectionBindings {
  camera: T.Camera;
  read(): PickingInput;
  actions: SelectionActions;
  hover(state: HoverState): void;
}
/** Only geometry and label values consumed by the renderer; no scene operations. */
export interface RenderView {
  nodes: ReadonlyMap<string, RenderNode>;
  hullIds: readonly string[];
  networkViews: readonly NetworkView[];
  fileViews: ReadonlyMap<string, FileView>;
  netlinkViews: ReadonlyMap<string, NetlinkView>;
  rdmaViews: readonly RdmaView[];
  rdmaLinkViews: readonly RdmaLinkView[];
  rdmaDeviceViews: readonly RdmaDeviceView[];
  /** Host floors of the merged view; absent or empty for a single host. */
  layerViews?: readonly HostLayerView[];
  edgeViews: readonly EdgeView[];
  baseGlow: T.InstancedMesh | null;
  haloGlow: T.InstancedMesh | null;
}
