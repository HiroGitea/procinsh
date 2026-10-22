// Pure hardware bay layout and labels: PCIe trees in front of the processes (-y).
import { Display } from "../shared/display.js";
import type {
  DeviceHandle,
  HardwareDevice,
  HardwarePath,
  PcieLink,
} from "../shared/api-types.js";
import type { Position } from "./types.js";

/** Distance from the front process row to the device row; boxes leave a small aisle. */
export const HARDWARE_GAP = 3.5;
/** Distance between tree levels, from devices back toward host bridges. */
export const HARDWARE_ROW = 3.5;
/** Distance between neighboring devices on a row. */
export const HARDWARE_COLUMN = 4.8;

/** Usable bandwidth in GB/s after line encoding. */
export const pcieBandwidth = (link: PcieLink) =>
  (link.speed_gts * (link.speed_gts <= 5 ? 0.8 : 128 / 130) * link.width) / 8;

/**
 * Lays out PCIe trees with devices nearest the processes and host bridges
 * farthest, each parent centered over its children. Devices keep previous
 * positions until the layout is rearranged.
 */
export function hardwareLayout(
  devices: readonly HardwareDevice[],
  processes: Iterable<Pick<Position, "x" | "y">>,
  previous: ReadonlyMap<string, Position> = new Map(),
) {
  const result = new Map<string, Position>();
  if (!devices.length) return result;
  if (devices.every((d) => previous.has(d.address))) {
    for (const d of devices) result.set(d.address, previous.get(d.address)!);
    return result;
  }
  let minX = Infinity,
    maxX = -Infinity,
    minY = Infinity;
  for (const p of processes) {
    minX = Math.min(minX, p.x);
    maxX = Math.max(maxX, p.x);
    minY = Math.min(minY, p.y);
  }
  if (!Number.isFinite(minY)) minX = maxX = minY = 0;
  const children = new Map<string | null, HardwareDevice[]>();
  const known = new Set(devices.map((d) => d.address));
  for (const d of devices) {
    const parent = d.parent !== null && known.has(d.parent) ? d.parent : null;
    children.set(parent, [...(children.get(parent) ?? []), d]);
  }
  for (const list of children.values())
    list.sort((a, b) => a.address.localeCompare(b.address));
  // Depth below the root bridge and the deepest leaf decide each row.
  const depth = new Map<string, number>();
  const visit = (parent: string | null, level: number) => {
    for (const d of children.get(parent) ?? []) {
      depth.set(d.address, level);
      visit(d.address, level + 1);
    }
  };
  visit(null, 0);
  const deepest = Math.max(...depth.values());
  const rowY = (address: string, leaf: boolean) =>
    minY -
    HARDWARE_GAP -
    (leaf ? 0 : (deepest - depth.get(address)!) * HARDWARE_ROW);
  let column = 0;
  const x = new Map<string, number>();
  const place = (parent: string | null) => {
    for (const d of children.get(parent) ?? []) {
      const below = children.get(d.address) ?? [];
      if (!below.length) x.set(d.address, column++ * HARDWARE_COLUMN);
      else {
        place(d.address);
        const xs = below.map((c) => x.get(c.address)!);
        x.set(d.address, (Math.min(...xs) + Math.max(...xs)) / 2);
      }
    }
  };
  place(null);
  const width = (column - 1) * HARDWARE_COLUMN;
  const shift = (minX + maxX) / 2 - width / 2;
  for (const d of devices)
    result.set(d.address, {
      x: x.get(d.address)! + shift,
      y: rowY(d.address, !(children.get(d.address) ?? []).length),
      z: 0,
    });
  return result;
}

/** The link runs slower than both ends allow, such as an idle GPU or a narrow slot. */
export function downgraded(
  device: HardwareDevice,
  parent: HardwareDevice | undefined,
) {
  const current = device.link?.current,
    max = device.link?.max;
  if (!current || !max) return false;
  const limit = parent?.link?.max;
  const speed = Math.min(max.speed_gts, limit?.speed_gts ?? Infinity),
    width = Math.min(max.width, limit?.width ?? Infinity);
  return current.speed_gts < speed || current.width < width;
}

export const pathLabel = (path: HardwarePath) =>
  `${path.class.toUpperCase()}${path.bottleneck ? ` · ${Display.pcieLink(path.bottleneck)}` : ""}`;

/** Users of each device, as process keys mapped to their handles. */
export function deviceUsers(
  handles: readonly DeviceHandle[],
  key: (handle: DeviceHandle) => string,
) {
  const users = new Map<string, Map<string, DeviceHandle[]>>();
  for (const handle of handles) {
    const byProcess = users.get(handle.device) ?? new Map();
    byProcess.set(key(handle), [...(byProcess.get(key(handle)) ?? []), handle]);
    users.set(handle.device, byProcess);
  }
  return users;
}
