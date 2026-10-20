# SPACE recording and replay

On the live `/space` page, select **Record**, continue exploring the machine, and
select **Stop & Download**. The browser downloads exactly one UTF-8 `.json`
file. Recording runs in memory, requires no upload or server storage, and starts
with the current complete snapshot, including maps retained from earlier deltas.

Recordings can contain process arguments, user names, paths and IP addresses.
Review them before sharing. The bundled **Chrome** and **Firefox** demos show real
browser activity captured in an Ubuntu 26.04 VM. Both open the same eight external
sites from [`scripts/browser_sites.txt`](../scripts/browser_sites.txt), with a
reload every ten seconds. **Linux kernel build** shows 30 seconds of real clang
compilation in a dedicated Ubuntu 26.04 QEMU/KVM guest.

A stream gap, reconnect, hidden tab or page exit ends the recording. The retained
recording stays available as **Download recording** when the page is visible
again. The UI reports the reason; it does not resume across missing observations.
Leaving or reloading with an undownloaded recording prompts the browser's
unsaved-work confirmation. Download before closing the tab: memory is not durable
storage. At 128 MiB the recorder stops and retains the completed prefix; local
replay uses the same size limit. The live view continues independently.

**Open recording** opens `/replay/space/`. Select or drop the original downloaded
JSON or a standard gzip-compressed `.json.gz`. Files are streamed with the browser
File API and never uploaded. Both the input and decompressed data are limited to
128 MiB; malformed gzip and invalid recordings are rejected. The loader inspects
the gzip magic bytes, so HTTP `Content-Encoding: gzip` decoding does not cause
double decompression. Choose another
file on the same page. Each recording starts playing automatically with **Loop**
enabled after it loads, including bundled demos opened by URL. Play/pause,
timeline seeking and 0.1×/0.25×/0.5×/1×/2× speeds use a virtual clock shared with
SPACE activity animation. Orbit, pan and zoom remain available while paused.
Hiding the replay tab pauses playback. Disable **Loop** to stop at the end.
Opening another recording enables loop playback again. Process inspection links
are disabled in replay.

## JSON contract

Version 1 is one JSON object containing:

- `format_version: 1`, `view_type: "space"` and ISO `started_at`.
- `duration_ms` and `end_reason`.
- `initial_snapshot`: complete `SystemSnapshot` (processes with complete maps and
  FD relations); no earlier SSE messages are required.
- `events`: ordered `{ at_ms, type, data }` objects. Time is milliseconds relative
  to recording start; equal timestamps retain array order. `type` is `snapshot`
  (a complete merged `SystemSnapshot`) or `activity` (`SpaceActivity`).

Payload definitions are in `web/src/shared/api-types.ts`; the shared runtime
parser is in `web/src/space/recording.ts`. Unknown versions, other view types,
invalid nested values and out-of-order/out-of-duration timestamps are rejected
before rendering. Local files and public recordings use this parser and the
same replay engine. Playback, seeking and looping end at the last event timestamp
(or zero for snapshot-only recordings). The timeline accepts fractional
milliseconds so its right edge reaches that exact timestamp. `duration_ms` retains
the elapsed recording time, which can include an interval between the last event
and stopping the recording. Seeking reconstructs state from the initial snapshot; this
bounds format complexity but can take longer for large recordings.

## Static site and Cloudflare Pages Free

The replay site builds with Node.js >=22.12 and npm alone:

```sh
npm --prefix web ci
npm --prefix web run build:replay
```

Publish **`web/dist-replay/`** directly. The build includes
`replay/space/index.html`, shared SPACE assets and the bundled `.json`/`.json.gz` recordings.
It contains no LIST/PROCESS/LIVE page. Direct navigation and refresh work through
the directory's `index.html`; `/replay/` is reserved for a future selector.
The viewer never opens SSE or calls live APIs.

To configure a Cloudflare Pages Git integration, choose the Free plan, connect
this repository, leave the root directory at the repository root, select no
framework preset, set the build command to
`npm --prefix web ci && npm --prefix web run build:replay`, and set the output
directory to `web/dist-replay`. Use Node.js 22.12 or newer.
No Worker, Pages Function, Rust toolchain, collector, privileged access, backend,
or paid service is involved. The documentation publishing workflow is independent.
Creating the Cloudflare account/project and its initial deployment requires the
owner's Cloudflare account; a local build does not provision those resources.

Select a bundled recording and click **Open demo** in either the live SPACE or
replay menu. Opening a demo starts looping playback automatically. The picker
and URL allowlist share the `bundledRecordings` catalog in
`web/src/space/session-controls.ts`; add an ID, label, and asset path there when
adding a recording to `web/replay-public/recordings/`.

Open `/replay/space/?recording=chrome-ubuntu26` on the deployed site. `recording` is an
allowlisted ID, never a user-supplied fetch URL. All bundled recordings are shared by the normal embedded build and static build.
Use `?recording=firefox-ubuntu26` for the Firefox workload. Synthetic data is
retained only as a private test fixture, outside the published asset directories. The build rejects
any static asset larger than 25 MiB, including public demos, according to
[Cloudflare Pages Free limits](https://developers.cloudflare.com/pages/platform/limits/).
No chunk conversion or companion files are needed.

For a deployment under a path prefix, build with
`PROCINSH_REPLAY_BASE=/prefix/ npm --prefix web run build:replay` and serve the
output under that prefix. Asset URLs and the demo fetch respect that base.

The normal `npm --prefix web run build` still outputs `web/dist/` for embedding
in Rust, with live `/space` and `/replay/space/` as separate page routes.

## Capturing browser demos

The VM scripts require a Linux x86-64 host with read/write access to `/dev/kvm`,
QEMU (`qemu-system-x86` and `qemu-utils`), `cloud-image-utils`, curl and Python 3.
If needed, add your user to the `kvm` group with `sudo usermod -aG kvm "$USER"`
and log out and back in.

```sh
# The guest clones the published commit from the current checkout.
# BROWSER accepts chrome (default) or firefox.
BROWSER=firefox VM_DIR=/tmp/space-demo scripts/recording-vm/run.sh prepare
VM_DIR=/tmp/space-demo scripts/recording-vm/run.sh run
```

The guest installs procinsh and the selected browser, then starts the collector
and browser workload. Both browsers use the same Puppeteer WebDriver BiDi calls
for tab creation, cache disabling, navigation and reload. They use fresh profiles
and the shared site list in `scripts/browser_sites.txt`.

Wait for `SPACE recording ready on guest port 10090` in
`/tmp/space-demo/console.log`, then open `http://127.0.0.1:10090/space` and use
**Record** / **Stop & Download**. The host and guest share port 10090; set
`HOST_PORT` during `prepare` to choose another port. `run` retains that port.
`VM_CPUS`, `VM_MEMORY_MIB` and `VM_DISK_GIB` control VM resources.

To capture automatically through the same browser controls:

```sh
(cd web && BROWSER=firefox HOST_PORT=10090 RECORD_SECONDS=15 \
  RECORD_OUTPUT=/tmp/firefox-vm.json.gz node --import tsx scripts/capture-vm-recording.ts)
```

The host needs Google Chrome and `npm --prefix web ci`. `CHROME` overrides its
executable path. The capture script saves a gzip recording and checks activity,
file size and playback. To compress a manual download, use
`gzip -n -9 recording.json`. Copy the demo to `web/replay-public/recordings/` and
add its entry in `web/src/space/session-controls.ts`.

For the guest console, run
`VM_DIR=/tmp/space-demo python3 scripts/recording-vm/console.py` (Ctrl-] disconnects).
Use `journalctl -u browser-demo` to inspect the workload. `TAB_COUNT`,
`INTERVAL_SECONDS` and `SITE_LIST` can be set in the `browser-demo` systemd service.
Stop the VM with `VM_DIR=/tmp/space-demo scripts/recording-vm/run.sh stop` and wait
for QEMU to exit before restarting it.

## Capturing the Linux kernel build demo

The kernel workload uses the same VM tools and browser recorder as the browser
workloads. `WORKLOAD` accepts `browser` (default) or `kernel-build`; `BROWSER`
continues to select Chrome or Firefox for the browser workload. Kernel guests
install no browser and run no browser workload.

```sh
WORKLOAD=kernel-build VM_DIR=/tmp/kernel-demo HOST_PORT=10191 \
  scripts/recording-vm/run.sh prepare
VM_DIR=/tmp/kernel-demo scripts/recording-vm/run.sh run
# Wait for the ready message in /tmp/kernel-demo/console.log.
(cd web && WORKLOAD=kernel-build HOST_PORT=10191 \
  RECORD_OUTPUT=/tmp/linux-build-ubuntu26.json.gz \
  node --import tsx scripts/capture-vm-recording.ts)
```

The build uses the exact Ubuntu source package corresponding to the running
guest's `linux-modules-$(uname -r)` package. Preparation enables Ubuntu source
repositories from the [Ubuntu Snapshot Service](https://snapshot.ubuntu.com/),
reads the source name and version with `dpkg-query`, downloads that
specific version through APT, and extracts its `.dsc` with `dpkg-source`. There is
no fallback to another version if the matching source is unavailable.
`KERNEL_SOURCE_SNAPSHOT` selects the source archive timestamp during `prepare`
(default `20260927T000000Z`, matching the default cloud image). Set it to a date
containing the guest kernel's source when using a different cloud image.

Following the [reference article](https://akawashiro.com/articles/linux-build-script),
compilation uses `CC=clang`, `defconfig` then `kvm_guest.config`, separate
source and output trees, and `max(1, nproc - 2)` jobs. Source is in
`/home/demo/linux`, downloaded archives are in `/home/demo/kernel-source`, and
output is in `/home/demo/linux-build`. The source changelog must match the
installed source package version. Following Ubuntu's packaging rules,
`KERNELRELEASE=$(uname -r)` preserves the guest's ABI and flavor suffix; Ubuntu's
stable patch level can differ from its release name. The upstream major/minor
series and configured `kernelrelease` are checked against the guest; a mismatch
stops preparation or build.
The kernel is not installed and the guest is not rebooted into it.

Defaults are 4 vCPUs, 8192 MiB RAM and a 40 GiB disk. Source download and
configuration finish before the workload service starts. Inspect
`journalctl -u kernel-build-demo`, `/home/demo/kernel-build.log`, and
`/home/demo/kernel-build-info.txt` through the serial console. The info file
records the guest and build releases, source package/version, compiler version,
job count, source descriptor SHA256 and config SHA256; the
complete configuration remains in `linux-build/.config`.

The capture command waits for an active clang process and a positive compiler
CPU delta in the live stream, then records 30 seconds by default. It checks
compiler CPU and file activity across all recorded snapshots.
It rejects interrupted recordings and validates gzip replay before saving.
`RECORD_SECONDS` overrides duration; browser workloads still default to 15 seconds.
Record during the initial build. For another clean recording, prepare a fresh VM
so the build output is empty. If a recording exceeds 128 MiB JSON
or 25 MiB gzip, use fewer `VM_CPUS` in a fresh guest and capture 30 seconds again.

Copy the validated gzip into `web/replay-public/recordings/`. The bundled demo ID
is `linux-build-ubuntu26`, available through the demo menu, static landing page,
and `/replay/space/?recording=linux-build-ubuntu26`. Stop the disposable VM after
recording using `VM_DIR=/tmp/kernel-demo scripts/recording-vm/run.sh stop`.

### Bundled kernel recording provenance

| Property | Value |
| --- | --- |
| Capture start (UTC) | `2026-10-20T04:25:40.568Z` |
| Duration / last event | 30.852 s / 30.516 s |
| Guest / resources | Ubuntu 26.04, `7.0.0-34-generic`, 4 vCPUs, 8192 MiB RAM, 40 GiB disk |
| procinsh revision | `eaff2f3a6f7df7ac16d7160e1cba2acdd6439d5e` |
| Ubuntu source package | `linux 7.0.0-34.34` |
| Source archive snapshot | `20260927T000000Z` |
| Source descriptor SHA256 | `03239d912d2204cee96216f4006d654ec3d9b4d8882488ca9200c26196c641fc` |
| Upstream stable patch level | `7.0.14` |
| Built kernel release | `7.0.0-34-generic` |
| Compiler / parallelism | Ubuntu clang 21.1.8 (`6ubuntu1`), `CC=clang`, 2 jobs |
| Configuration | `defconfig` + `kvm_guest.config`, `KERNELRELEASE=7.0.0-34-generic` |
| Config SHA256 | `1ce366d499bd18f8655672bdcf392e21ee72f5265c6122a6934eae3f024b1877` |
| JSON / gzip bytes | 65,805,411 / 4,350,131 |
| Gzip SHA256 | `e5569419a48b17b8885761d7ae65cb72bd1d1d13350bc319c250fec751e47ca2` |

The recording contains 51 distinct compiler identities, 2 compiler CPU samples
with positive runtime, and 28,853 compiler file-activity samples. It is an excerpt
of compilation, not a recording of a completed build.

## Validation

Run `bash scripts/docker_test.sh ordinary`, `checks` and `browser`.
