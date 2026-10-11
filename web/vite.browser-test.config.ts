import { resolve } from "node:path";
import { defineConfig, mergeConfig, type Plugin } from "vite";
import applicationConfig from "./vite.config.js";

// Append inspection exports only when building the browser regression fixture.
const inspections = new Map([
  [
    resolve(import.meta.dirname, "src/space/app.ts"),
    `
const cpuGlowStates = data.cpuGlows;
const cpuGlowVisual = scene.cpuGlowVisual,
  parentLineVisual = scene.parentLineVisual;
const networkVisuals = renderer.networkVisuals,
  networkParticles = renderer.networkParticles,
  signalVisuals = renderer.signalVisuals,
  fileVisuals = renderer.fileVisuals,
  fileParticles = renderer.fileParticles;
const cameraView = camera.view;
function processPosition(id: string) {
  const pos = data.nodes.get(id)?.pos;
  return pos && { x: pos.x, y: pos.y, z: pos.z };
}
function previewFamilyRoutes(legacy: boolean) {
  const parentRoutes = legacy ? data.parentRoutes.map(route => {
    const a = data.nodes.get(route.parent)!.pos,
      b = data.nodes.get(route.child)!.pos,
      y = (a.y + b.y) / 2;
    return { ...route, points: [{x:a.x,y:a.y},{x:a.x,y},{x:b.x,y},{x:b.x,y:b.y}] };
  }) : data.parentRoutes;
  scene.rebuild({ ...sceneInput(), parentRoutes }, selection.process);
  scene.updateSelection(selection);
}
function connectionVisual(id: string) {
  const view = scene.renderView().edgeViews.find(view => view.e.id === id);
  return view && {
    points: view.curve.getPoints(16).map(point => point.toArray()),
    screen: view.curve.getPoint(0.75).project(camera.camera).toArray(),
  };
}
export {
  selectFile,
  fileVisuals,
  fileParticles,
  pruneFiles,
  renderSystemSnapshot,
  renderActivity,
  fitScene,
  selectProcess,
  selectConnection,
  selectNetwork,
  networkVisuals,
  networkParticles,
  signalVisuals,
  cameraView,
  processPosition,
  previewFamilyRoutes,
  connectionVisual,
  parentLineVisual,
  cpuGlowStates,
  cpuGlowVisual,
};

export { key } from "./model.js";
export { treeLayout } from "./data.js";
export { AdaptiveRenderScale } from "./renderer.js";
`,
  ],
  [resolve(import.meta.dirname, "src/space/data.ts"), "export { treeLayout };"],
  [
    resolve(import.meta.dirname, "src/space/renderer.ts"),
    "export { AdaptiveRenderScale };",
  ],
]);

export default defineConfig((environment) =>
  mergeConfig(applicationConfig(environment), {
    plugins: [
      {
        name: "space-browser-inspections",
        enforce: "pre",
        transform(source, id) {
          const inspection = inspections.get(id);
          return inspection
            ? { code: `${source}\n${inspection}`, map: null }
            : undefined;
        },
      } satisfies Plugin,
    ],
    build: {
      rolldownOptions: {
        input: { spaceApp: resolve(import.meta.dirname, "src/space/app.ts") },
        preserveEntrySignatures: "exports-only",
      },
    },
  }),
);
