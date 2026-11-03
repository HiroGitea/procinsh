# SPACE scene construction

`web/src/space/scene.ts` exposes `createSpaceScene`. It composes geometry builders,
keeps the current rendering and picking references, applies selection, and owns
all scene resources. Its public interface is defined by `contracts.ts`.

- `scene-hardware.ts`: hardware devices, PCIe ribbons, process/device links, and selection overlays.
- `scene-rdma.ts`: RDMA device plates, queue-pair markers, and links.
- `scene-layers.ts`: host floors and label anchors.

Builders receive the current input, process nodes, and a caller-owned group. They
append geometry directly to that group and return rendering references; they do
not own subscriptions or dispose resources. Hardware, RDMA, and host floors are
built in that order, followed by file geometry. Hardware overlays are rebuilt on
selection updates using the current nodes and the last hardware input.

Rebuilds dispose old geometry immediately but retain its materials until the
next render completes. This allows replacements to reuse shader programs.
Final scene disposal also releases any materials still awaiting a render.

Docker `ordinary` covers scene construction, picking, selection, and disposal;
`browser` covers the live SPACE page and static recording replay.
