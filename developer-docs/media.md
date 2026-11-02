# Media sources

The README images in `images/` and bundled demos in
`web/replay-public/recordings/` show procinsh built from main commit
`a6ada90247a6edefc61f015017b8e11c3e2fc3e3`.

All collectors, workloads, and capture browsers run inside disposable x86-64
QEMU/KVM virtual machines. Each guest uses the Ubuntu 26.04 cloud image
`release-20260927`, four vCPUs, 8 GiB RAM, and a 40 GiB virtual disk. The image
is verified against its published SHA256 before boot. Guests have no host
filesystem mounts or host credentials.

| Asset | Source |
| --- | --- |
| `procinsh_space.png` | Live Chromium VM, all processes |
| `procinsh_space_chromium.png` | Live Chromium VM, selected Chromium process |
| `procinsh_process_chromium.png` | Live Chromium process details, including sampled registers and disassembly |
| `procinsh_list.png` | Live Chromium VM process list |
| `procinsh_movie.gif` | Live Chromium VM, Tour mode |
| `procinsh_multi_host.png` | Chromium VM hub with Firefox and Linux build VM agents, All hosts (layers) |
| `chromium-ubuntu26.json.gz` | Chromium browser workload |
| `firefox-ubuntu26.json.gz` | Firefox browser workload |
| `linux-build-ubuntu26.json.gz` | Upstream Linux v7.0 compilation with clang, two parallel jobs |

Screenshots use a 1600 × 1100 viewport; process details include the full page. The GIF uses 96 frames at eight frames
per second, scaled to 800 pixels wide with an ffmpeg palette.

Prepare guests with `scripts/recording-vm/run.sh` and start procinsh through
`scripts/dev_run.sh`. Install Chromium in each guest that drives a capture;
install ffmpeg in the guest generating the GIF. Run
`web/scripts/capture-vm-recording.ts` inside the guest, with `HOST_PORT` set to
that guest's collector port. Browser recordings last 15 seconds; the Linux
build recording lasts 30 seconds. The recorder verifies real workload CPU,
IPC, and file activity (CPU and files for compilation), size limits, and
successful gzip replay before writing an asset.

For the multi-host view, connect the two agent guests to the Chromium guest's
hub through QEMU's forwarded agent port. Use a disposable demo token created
for these guests. Capture `/space?host=all` after `/api/hosts` reports all three
hosts online. Transfer finished assets through a temporary guest HTTP server;
stop that server and the disposable guests after review.
