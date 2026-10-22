// Inputs and callbacks for SPACE geometry, picking, details, and rendering.
import type * as T from "three";
import type {
  DeviceHandle,
  HardwareDevice,
  HardwarePath,
  SystemSnapshot,
} from "../shared/api-types.js";
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
  HardwarePathView,
  HardwareView,
} from "./types.js";

export interface SelectionActions {
  process(id: string | null, focus?: boolean): void;
  connection(id: string | null): void;
  network(id: string | null): void;
  file(id: string | null): void;
  netlink(id: string | null): void;
  hardware?(id: string | null): void;
  clear(): void;
}
export interface SelectionTargets {
  processes: ReadonlySet<string>;
  connections: ReadonlySet<string>;
  networks: ReadonlySet<string>;
  files: ReadonlySet<string>;
  netlink: ReadonlySet<string>;
  hardware?: ReadonlySet<string>;
}
/** Devices, paths and descriptor holders of the hardware bay. */
export interface HardwareInput {
  devices: readonly HardwareDevice[];
  paths: readonly HardwarePath[];
  handles: readonly DeviceHandle[];
}
export interface FileSceneInput {
  nodes: ReadonlyMap<string, PlacedProcess>;
  files: ReadonlyMap<string, RecentFile>;
  netlink: ReadonlyMap<string, NetlinkSocketStats>;
  filePositions: ReadonlyMap<string, Position>;
  netlinkPositions: ReadonlyMap<string, Position>;
  visible: ReadonlySet<string>;
}
export interface SceneInput extends FileSceneInput {
  snapshot: SystemSnapshot;
  network: ReadonlyMap<string, NetworkGroup>;
  networkPositions: ReadonlyMap<string, Position>;
  hardware?: HardwareInput & { positions: ReadonlyMap<string, Position> };
}
export interface WorldPositions {
  processes: Iterable<Position>;
  networks: Iterable<Position>;
  files: Iterable<Position>;
  netlink: Iterable<Position>;
}
export interface DetailsInput {
  nodes: ReadonlyMap<string, PlacedProcess>;
  hardware?: HardwareInput;
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
export interface PickingInput {
  view: PickingView;
  hardware?: HardwareInput;
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
  hardwareViews?: readonly HardwareView[];
  hardwarePathViews?: readonly HardwarePathView[];
  edgeViews: readonly EdgeView[];
  baseGlow: T.InstancedMesh | null;
  haloGlow: T.InstancedMesh | null;
}
