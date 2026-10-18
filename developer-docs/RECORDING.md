# SPACE recording and replay

On the live `/space` page, select **Record**, continue exploring the machine, and
select **Stop & Download**. The browser downloads exactly one UTF-8 `.json`
file. Recording runs in memory, requires no upload or server storage, and starts
with the current complete snapshot, including maps retained from earlier deltas.

Recordings can contain process arguments, user names, paths and IP addresses.
Review them before sharing. Demo #1 uses synthetic observations. **Chrome** contains real
observations from a dedicated VM; its provenance, sizes, memory samples and
publication review are in [the recording manifest](recordings/chrome-ubuntu26/README.md).

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
directory to `web/dist-replay`. Use Node.js 22.12 or newer. The root
`wrangler.jsonc` also declares the Pages output directory for direct upload.
No Worker, Pages Function, Rust toolchain, collector, privileged access, backend,
or paid service is involved. The documentation publishing workflow is independent.
Creating the Cloudflare account/project and its initial deployment requires the
owner's Cloudflare account; a local build does not provision those resources.

Select a bundled recording and click **Open demo** in either the live SPACE or
replay menu. Opening a demo starts looping playback automatically. The picker
and URL allowlist share the `bundledRecordings` catalog in
`web/src/space/session-controls.ts`; add an ID, label, and asset path there when
adding a recording to `web/replay-public/recordings/`.

Open `/replay/space/?recording=demo` on the deployed site. `recording` is an
allowlisted ID, never a user-supplied fetch URL. Both bundled recordings are shared by the normal embedded build and static build.
Use `?recording=chrome-ubuntu26` for the real Chrome workload. The build rejects
any static asset larger than 25 MiB, including public demos, according to
[Cloudflare Pages Free limits](https://developers.cloudflare.com/pages/platform/limits/).
No chunk conversion or companion files are needed.

For a deployment under a path prefix, build with
`PROCINSH_REPLAY_BASE=/prefix/ npm --prefix web run build:replay` and serve the
output under that prefix. Asset URLs and the demo fetch respect that base.

The normal `npm --prefix web run build` still outputs `web/dist/` for embedding
in Rust, with live `/space` and `/replay/space/` as separate page routes.

## Validation

Run `bash scripts/docker_test.sh ordinary`, `checks` and `browser`. Ordinary tests
include the standalone build, schema validation, midstream recording with merged
maps, relative timestamps, event ordering, playback/seek/step and asset size
boundaries. Browser tests cover recording download, unchanged local JSON picker
and drop loading, and a replay-only static server under `/demo/`, including
refresh, slow playback, unknown recording IDs and zero live API requests.

## Dedicated Ubuntu 26.04 Chrome recording VM

Use a Linux x86-64 host with readable/writable `/dev/kvm`, QEMU (`qemu-system-x86`
and `qemu-utils`), `cloud-image-utils`, curl and Python 3. The scripts use KVM,
virtio disks and networking, a QCOW2 Ubuntu Cloud Image and a NoCloud seed. They
use QEMU directly, with no libvirt, Vagrant, TAP, bridge, SSH tunnel, shared host
directory or host credentials. The default image is the dated Ubuntu 26.04 build
`release-20260927`; its SHA256 is checked against the official HTTPS checksum
list. `IMAGE_BASE` selects another dated official image directory.

The invoking user must have read/write access to `/dev/kvm`. On hosts where the
device belongs to the `kvm` group, add the user with
`sudo usermod -aG kvm "$USER"`, then log out and back in before starting QEMU.
Check access with `test -r /dev/kvm && test -w /dev/kvm`. QEMU runs directly as
that user; no container or root QEMU process is required. Disk preparation needs
`qemu-img`, `cloud-localds` and curl; starting a prepared guest only needs
`qemu-system-x86_64`, Python 3 and flock (from util-linux).

```sh
# The commit must be published so the guest can clone it.
VM_DIR=/tmp/space-demo scripts/recording-vm/run.sh prepare
VM_DIR=/tmp/space-demo HOST_PORT=10090 VM_CPUS=4 VM_MEMORY_MIB=8192 \
  scripts/recording-vm/run.sh run
```

`prepare` refuses to overwrite an existing guest. `PROCINSH_REF` defaults to the
checkout's full commit SHA; `VM_DISK_GIB` defaults to 40. `HOST_PORT` defaults to
10090 and sets the same HTTP port on both the host and guest during `prepare`.
The selected port is retained in `http-port`; `run` uses it automatically. To use
a different port, prepare a new VM directory with `HOST_PORT=...`. `run` rejects a
conflicting `HOST_PORT` override. `run` checks the requested
port before starting QEMU and reports an explicit error if it is busy. QEMU also
checks the bind, covering a race after the preliminary check. Its forwarding is
`hostfwd=tcp:127.0.0.1:${HOST_PORT}-:${HOST_PORT}`; the host's LAN is not exposed. CPU and
memory settings are saved to `resources.txt` and QEMU's version to
`qemu-version.txt`. The state directory must not contain commas or whitespace.

Cloud-init installs build dependencies, the repository's pinned Rust toolchain,
Node.js and Google Chrome, builds procinsh, and starts two systemd services:

- `space-demo`: the privileged collector on guest `0.0.0.0:HOST_PORT` (default
  10090), using `scripts/dev_run.sh --allow-non-loopback`. Cloud-init configures
  its systemd service with the same port as the host forwarding rule. Numeric
  host, port, Origin and Fetch Metadata checks remain enabled.
- `chrome-demo`: an unprivileged Chrome controller, eight tabs, ten-second
  reloads. Each tab has a distinct local origin, a Web Worker, periodic 64 KiB
  loopback fetches and IndexedDB writes. No external website is required.

The unauthenticated collector is intended only for this disposable VM. Do not
copy personal data, secrets or credentials into it. Initial installation needs
Internet access. Chrome background networking is disabled for the local workload.
The image, repository SHA, installed packages, kernel, Rust, Node.js, Chrome and
bpftool versions and kernel BPF feature probe are saved in the guest's
`/opt/recording-metadata/`. Apt/Rust/Chrome downloads are not immutable mirrors;
retain that manifest when regenerating recordings to identify version drift.

Follow `tail -f /tmp/space-demo/console.log` and wait for
`SPACE recording ready on guest port 10090`. Connect an interactive serial console
with `VM_DIR=/tmp/space-demo python3 scripts/recording-vm/console.py` (Ctrl-]
disconnects). Setup
can take several minutes. Open `http://127.0.0.1:10090/space` on the host, select
**Record**, and then **Stop & Download**. Compress the download with standard
`gzip -n -9 recording.json`. Keep the JSON below 128 MiB and gzip below 25 MiB
(target 20 MiB). Open the `.json.gz` in `/replay/space/` to verify it.

For an automated host-browser capture through the same Record/Download controls:

```sh
(cd web && HOST_PORT=10090 RECORD_SECONDS=15 RECORD_OUTPUT=/tmp/chrome-vm.json.gz \
  node --import tsx scripts/capture-vm-recording.ts)
```

The host needs Google Chrome (`CHROME` can override its path). The script also
loads the resulting gzip in the VM's replay page, checks that it plays, and writes
compressed/uncompressed sizes and browser JavaScript heap samples to
`/tmp/chrome-vm.json.gz.metrics.json`. Heap measurements are JavaScript allocations,
not total Chrome/OS memory; retain VM resource settings alongside them. Adjust
duration or tab count if recording approaches the 128 MiB limit. Change tab count
and reload cadence with a `sudo systemctl edit chrome-demo` Environment override via
the guest console; use `journalctl -u chrome-demo` to inspect workload failures.

The serial console automatically logs in as the locked-password `demo` user;
password authentication is disabled and no SSH port is forwarded. To shut down cleanly, in another host terminal:

```sh
VM_DIR=/tmp/space-demo scripts/recording-vm/run.sh stop
```

Wait for QEMU to exit before restarting or removing the state directory.
Terminating the QEMU process immediately can leave an unclean guest filesystem.
Inspect `/var/log/recording-setup.log` and `journalctl -u space-demo` on failures.
Before publishing a recording, review process arguments, usernames, paths and
network addresses, and retain process/IPC identities if sanitization is needed.
Only then copy the gzip to `web/replay-public/recordings/` and add its catalog entry
in `web/src/space/session-controls.ts`. The standalone build checks every source
asset and every generated file against the Pages Free 25 MiB maximum.
