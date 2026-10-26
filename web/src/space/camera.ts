// Own camera projection, world bounds, focus, and OrbitControls.
import * as T from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { Position } from "./types.js";
import type { WorldPositions } from "./contracts.js";
export function createSpaceCamera(canvas: HTMLCanvasElement) {
  const camera = new T.PerspectiveCamera(
    45,
    innerWidth / innerHeight,
    0.1,
    1500,
  );
  camera.up.set(0, 0, 1);
  camera.position.set(65, -85, 70);
  const controls = new OrbitControls(camera, canvas);
  controls.enableDamping = true;
  controls.dampingFactor = 0.07;
  controls.maxPolarAngle = Math.PI * 0.49;
  controls.minDistance = 0.35;
  controls.maxDistance = 500;
  // OrbitControls scales panning by camera distance. Keep close-up navigation usable.
  function updatePanSpeed() {
    controls.panSpeed = Math.max(
      1,
      40 /
        Math.max(
          controls.minDistance,
          camera.position.distanceTo(controls.target),
        ),
    );
  }
  controls.addEventListener("start", updatePanSpeed);
  controls.addEventListener("change", updatePanSpeed);

  function worldBounds(positions: WorldPositions) {
    const box = new T.Box3();
    for (const pos of positions.processes) {
      box.expandByPoint(new T.Vector3(pos.x, pos.y, pos.z));
      box.expandByPoint(new T.Vector3(pos.x, pos.y, 8));
    }
    for (const p of positions.networks)
      box.expandByPoint(new T.Vector3(p.x, p.y, p.z + 1));
    for (const p of [...positions.files, ...positions.netlink])
      box.expandByPoint(new T.Vector3(p.x, p.y, p.z - 1));
    return box;
  }
  function adaptWorld(positions: WorldPositions, expandOnly = false) {
    const box = worldBounds(positions);
    if (box.isEmpty()) return;
    const size = box.getSize(new T.Vector3()),
      extent = Math.max(50, size.x, size.y, size.z);
    controls.maxDistance = Math.max(
      500,
      extent * 2,
      expandOnly ? controls.maxDistance : 0,
    );
    camera.far = Math.max(1500, extent * 4, expandOnly ? camera.far : 0);
    camera.updateProjectionMatrix();
    return Math.min(0.0015, 0.8 / extent);
  }
  function fit(positions: WorldPositions) {
    const box = worldBounds(positions);
    if (box.isEmpty()) return;
    const center = box.getCenter(new T.Vector3()),
      size = box.getSize(new T.Vector3());
    const distance = Math.max(size.x, size.y, size.z) * 0.8 + 20;
    controls.target.copy(center);
    camera.position
      .copy(center)
      .add(new T.Vector3(distance * 0.55, -distance * 0.8, distance * 0.85));
  }
  function cameraView() {
    return {
      position: camera.position.toArray(),
      target: controls.target.toArray(),
    };
  }

  return {
    camera,
    controls,
    adaptWorld,
    fit,
    view: cameraView,
    /** Moves towards the orbit target by `factor` < 1, or away by > 1. */
    zoom(factor: number) {
      const offset = camera.position.clone().sub(controls.target);
      const distance = Math.min(
        controls.maxDistance,
        Math.max(controls.minDistance, offset.length() * factor),
      );
      camera.position.copy(controls.target).add(offset.setLength(distance));
      controls.update();
    },
    /**
     * Moves the camera and orbit target together over the ground plane:
     * `forward` along the view, `right` across it, in world units.
     */
    move(forward: number, right: number) {
      const ahead = camera.getWorldDirection(new T.Vector3());
      ahead.z = 0;
      // Looking straight down, forward is the top of the screen.
      if (ahead.lengthSq() < 1e-6)
        ahead.set(0, 1, 0).applyQuaternion(camera.quaternion).setZ(0);
      ahead.normalize();
      const side = new T.Vector3().crossVectors(ahead, camera.up).normalize();
      const step = ahead.multiplyScalar(forward).addScaledVector(side, right);
      camera.position.add(step);
      controls.target.add(step);
    },
    /**
     * Orbits the camera around its target: `azimuth` radians counterclockwise
     * seen from above, `elevation` radians towards a top view, within the
     * controls' polar limits.
     */
    orbit(azimuth: number, elevation: number) {
      if (!azimuth && !elevation) return;
      const offset = camera.position.clone().sub(controls.target);
      const spherical = new T.Spherical().setFromVector3(
        new T.Vector3(offset.x, offset.z, -offset.y),
      );
      spherical.theta += azimuth;
      spherical.phi = Math.min(
        controls.maxPolarAngle,
        Math.max(
          Math.max(controls.minPolarAngle, 0.01),
          spherical.phi - elevation,
        ),
      );
      const turned = new T.Vector3().setFromSpherical(spherical);
      camera.position
        .copy(controls.target)
        .add(new T.Vector3(turned.x, -turned.z, turned.y));
      controls.update();
    },
    /** Distance from the camera to its orbit target. */
    distance() {
      return camera.position.distanceTo(controls.target);
    },
    focus(pos: Position) {
      controls.target.set(pos.x, pos.y, pos.z + 4);
      camera.position.set(pos.x + 13, pos.y - 20, pos.z + 15);
    },
    resize() {
      camera.aspect = innerWidth / innerHeight;
      camera.updateProjectionMatrix();
    },
    dispose() {
      controls.dispose();
    },
  };
}
