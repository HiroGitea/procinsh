# SPACE label profiling — issue #230

Collection date: 2026-10-17. See the [collection procedure](../../FRONTEND_PROFILING.md) and the scenario metadata beside the profiles.

## Conditions and artifacts

The baseline renderer is `7bb1b8a13cafbb0d6ca31b549dd507fdd8ed9036`; the collector is available at `ca9ca0c`. The comparison renderer is `e9038e6`. Source-map-enabled production assets were embedded in `cargo build --release --locked` binaries, launched through `scripts/dev_run.sh --release`. The SPACE bundle names in the profiles distinguish the baseline (`space-DzZFTMMs.js`) from the comparison (`space-CHVzDYEW.js`). Dirty-worktree fields reflect uncommitted documentation and, during baseline collection, source edits that were not embedded in the running baseline binary; collector metadata additions do not change the workload.

Hardware: AMD Ryzen 9 5950X, 32 logical CPUs; NVIDIA GeForce RTX 5060 Ti, NVIDIA driver 580.178.04; Chrome 141.0.7390.65, ANGLE OpenGL/EGL hardware rendering. Each scenario uses a fresh isolated headless Chrome, 1440 × 1100 CSS pixels, DPR 1, a visible document, 100% render scale, five seconds of settling after the initial FPS indicator, and a 15-second recording. The same synthetic fixture provides 256 processes, 4096 mappings, and 255 pipe relations on both sides. The busy case includes mapping permission changes, CPU activity, IPC bursts, and signals at the rates specified in the procedure. No builds/tests were run concurrently with the fixed-scenario recordings.

The first investigation recorded the standard Chrome CPU profiler and timeline against the live release backend before rendering changes. It pointed to label projection, `measureText`, and label drawing. The fixed scenarios then quantified these costs before and after the label change. The session had no interactive desktop: the DevTools profiler/timeline and process CPU counters were accessed through CDP, rather than operating the desktop DevTools UI or Task Manager. These are hardware-rendered headless measurements; desktop presentation behavior remains outside this comparison.

All four scenarios have raw V8 profiles, full metadata/summaries, and source maps checked in. The idle SPACE timelines are also included. The collector writes timelines for every scenario to `target/frontend-profile`; the other timelines are omitted here to limit repository size.

| Scenario | Baseline | Comparison |
| --- | --- | --- |
| Idle SPACE | [CPU profile](before/idle-space.cpuprofile.gz), [metadata](before/idle-space.json), [timeline](before/idle-space.trace.json.gz) | [CPU profile](after/idle-space.cpuprofile.gz), [metadata](after/idle-space.json), [timeline](after/idle-space.trace.json.gz) |
| Orbit/pan/zoom | [CPU profile](before/interact-space.cpuprofile.gz), [metadata](before/interact-space.json) | [CPU profile](after/interact-space.cpuprofile.gz), [metadata](after/interact-space.json) |
| Mapping/IPC load | [CPU profile](before/busy-space.cpuprofile.gz), [metadata](before/busy-space.json) | [CPU profile](after/busy-space.cpuprofile.gz), [metadata](after/busy-space.json) |
| Idle LIST | [CPU profile](before/idle-list.cpuprofile.gz), [metadata](before/idle-list.json) | [CPU profile](after/idle-list.cpuprofile.gz), [metadata](after/idle-list.json) |

Decompress a profile or timeline with `gunzip -k <file.gz>` and load the resulting file in DevTools Performance. Corresponding `*.map.gz` files retain the source maps; the metadata already maps the largest self-time samples to original source locations (zero-based lines).

## Findings

CPU percentages below use one logical core as 100%. The renderer counter includes its threads; main-thread task busy time is reported separately. Frame callback p95 is a main-thread timeline duration, not a GPU or presentation measurement.

| Scenario | Renderer CPU, before → after | Main-thread busy, before → after | Frame callback p95, before → after |
| --- | --- | --- | --- |
| Idle SPACE | 13.67% → 10.80% | 8.13% → 4.96% | 0.700 → 0.240 ms |
| Orbit/pan/zoom | 17.39% → 18.29% | 11.27% → 11.54% | 0.800 → 0.786 ms |
| Mapping/IPC load | 23.07% → 23.40% | 16.48% → 16.87% | 1.972 → 2.299 ms |
| Idle LIST | 7.13% → 7.20% | 3.72% → 3.82% | N/A (no animation callbacks) |

Idle SPACE renderer CPU falls by 2.87 percentage points (about 21% relative), and main-thread task busy time falls by 3.16 points (about 39% relative). Its frame callback p95 falls from 0.700 to 0.240 ms. All SPACE recordings contain 900–902 animation callbacks over approximately 15 seconds, and all eight recordings contain zero main-thread tasks longer than 50 ms. SPACE FPS endpoints are 59–60 FPS at 100% render scale on both sides.

The baseline idle CPU profile attributes approximately 347 ms of sampled inclusive time to `drawLabels` (including descendants) over the recording, versus 217 ms to Three.js rendering, 117 ms to `buildScene`, and 41 ms to `replaceSnapshot`. Label work accounts for about 29% of non-idle sampled time; it is the largest named JavaScript subtree in this case. Its `add` projection helper contributes 123 ms self time and native `measureText` 79 ms. The `(program)` samples remain unattributed native work. The idle timeline records only 26 ms of top-level GC, so collection pauses are not the dominant cost. In the comparison, sampled inclusive label time is 8.5 ms; rendering remains about 196 ms, scene construction 111 ms, and snapshot replacement 41 ms. These are sampled estimates, not function timers or GPU timings.

During camera interaction, labels still redraw and sampled inclusive label time is essentially unchanged (367 → 363 ms). The callback p95 is similar (0.800 → 0.786 ms), while renderer CPU rises from 17.39% to 18.29% in this single pair. Under busy signals/IPC, labels also keep animating: renderer CPU is 23.07% → 23.40%, and callback p95 rises from 1.972 to 2.299 ms. These cases provide no evidence of an interaction or loaded-scene CPU improvement, and the small increases require repeated pairs before attributing a regression. Their callbacks remain well below a 16.7 ms frame budget and no long tasks appear. The busy profile points mainly to particle/animation work in `renderer.ts` and Three.js curve/vector work, rather than GC or DOM layout.

LIST uses no animation loop in these recordings. Its renderer CPU is 7.13% → 7.20% and main-thread busy time is 3.72% → 3.82%; the list bundle is identical. Its samples point to periodic value formatting, DOM replacement, and unattributed native work. No LIST optimization is supported by this comparison. Layout/paint and GC distributions are retained in the metadata rather than being inferred from FPS.

## Scope and next measurements

The renderer retains the 2D label overlay until scene content, selection/hover, camera matrices, projection, or canvas size changes. Signal labels redraw while moving and once after expiry. WebGL rendering, OrbitControls damping, CPU glow, IPC particles, and signal effects retain their animation cadence. Structural snapshots still invalidate the overlay once per update; this comparison therefore includes continuous SSE traffic even in the idle fixture.

GPU execution time, power consumption, desktop frame presentation, and retained-heap growth were not measured. The `GPU` CPU counter in the summaries measures CPU used by Chrome's GPU process. It does not establish GPU utilization or a GPU performance improvement. Heap endpoints and GC show ordinary allocation/collection activity over short windows; they do not establish a leak or its absence. Use longer Memory recordings if retained growth is observed.

These are single 15-second recordings per scenario, without statistical significance claims. Host scheduling, profiler overhead, camera paths, scene distribution, and update timing affect the numbers. Use repeated pairs and a real workload before generalizing. The profile fixtures do not exercise process exits, gaps/resync, or arbitrary network/file/netlink topology; correctness for those cases is covered by the browser and ordinary suites. Full-scene reconstruction and continuous WebGL submission remain candidates for a further baseline-led investigation; the current measurement supports the narrower label change.
