// Build a closed route near several processes and sample it at constant world speed.
import * as T from "three";
import type { TourSubject } from "./types.js";

export const FLYTHROUGH_SECONDS = 15;
export const FLYTHROUGH_SPEED = 6;
export interface TourFlight {
  path: T.CatmullRomCurve3;
  length: number;
  offset: T.Vector3;
}

export function createTourFlight(
  subjects: readonly TourSubject[],
  angle: number,
  random = Math.random,
): TourFlight {
  const seed = subjects[Math.floor(random() * subjects.length)];
  const center = seed
    ? new T.Vector3(seed.position.x, seed.position.y, seed.position.z)
    : new T.Vector3(0, 0, 4);
  const nearby = subjects
    .map(({ position }) => new T.Vector3(position.x, position.y, position.z))
    .sort((a, b) => a.distanceToSquared(center) - b.distanceToSquared(center));
  const selected: T.Vector3[] = [];
  for (const point of nearby) {
    if (selected.every((p) => p.distanceToSquared(point) > 0.01))
      selected.push(point);
    if (selected.length === 6) break;
  }
  const points: T.Vector3[] = [];
  if (selected.length < 2) {
    for (let i = 0; i < 4; i++) {
      const a = (i * Math.PI) / 2;
      points.push(
        center.clone().add(new T.Vector3(Math.cos(a) * 5, Math.sin(a) * 5, 0)),
      );
    }
  } else {
    // Visit nearby subjects in nearest-neighbor order. Bow each leg sideways
    // so even two subjects or a collinear row allow smooth turns without stops.
    const ordered = [selected.shift()!];
    while (selected.length) {
      const last = ordered.at(-1)!;
      selected.sort(
        (a, b) => a.distanceToSquared(last) - b.distanceToSquared(last),
      );
      ordered.push(selected.shift()!);
    }
    for (let i = 0; i < ordered.length; i++) {
      const from = ordered[i],
        to = ordered[(i + 1) % ordered.length];
      const direction = to.clone().sub(from);
      const side = new T.Vector3(-direction.y, direction.x, 0);
      if (side.lengthSq() < 1e-6) side.set(1, 0, 0);
      side.normalize().multiplyScalar(Math.min(4, direction.length() * 0.35));
      points.push(from, from.clone().lerp(to, 0.5).add(side));
    }
  }
  const path = new T.CatmullRomCurve3(points, true, "centripetal");
  path.arcLengthDivisions = 4096;
  return {
    path,
    length: path.getLength(),
    offset: new T.Vector3(Math.cos(angle) * 14, Math.sin(angle) * 14, 6),
  };
}

export function flythroughTarget(
  flight: TourFlight,
  seconds: number,
  target: T.Vector3,
) {
  const distance = Math.max(0, seconds) * FLYTHROUGH_SPEED;
  return flight.path.getPointAt(
    (distance % flight.length) / flight.length,
    target,
  );
}
