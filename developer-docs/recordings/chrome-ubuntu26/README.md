# Chrome on Ubuntu 26.04

`web/replay-public/recordings/chrome-ubuntu26.json.gz` is a real SPACE
recording captured through the existing Record / Stop & Download controls from
an Ubuntu 26.04 QEMU/KVM guest. SLIRP forwards host port 10090 to guest port 10090.

Both browser recordings use the same eight external URLs in `sites.txt`: Mozilla,
Chromium, Python, Rust, Go, Node.js, TypeScript and kernel.org. Each tab reloads
every ten seconds with caching disabled. Both use the same Puppeteer WebDriver BiDi API for launch, tab creation,
cache disabling, navigation and reload. Navigation waits for DOMContentLoaded.
Puppeteer supplies the Chrome BiDi adapter; Firefox exposes native BiDi. The workload serves no local pages and generates no HTML.
`workload.log` records tab creation and reloads around the capture.

| Observation | Value |
| --- | --- |
| Recorded UTC | 2026-10-18T08:59:58.596Z |
| Duration | 15.6133 seconds |
| Guest | Ubuntu 26.04, 4 vCPUs, 8192 MiB RAM, 40 GiB QCOW2 |
| Initial Chrome processes | 18 |
| Processes across snapshots | 165 |
| Snapshot / activity events | 13 / 15 |
| Chrome CPU / IPC / file samples | 165 / 811 / 1015 |
| Original JSON / gzip bytes | 62,271,987 / 3,061,637 |
| Published JSON / gzip bytes | 62,262,279 / 3,075,488 |
| Published gzip SHA256 | `ef774889c1afd628357bec69867ac90227f3a685d4a1cc250777cfd70f43ae62` |

The published gzip is below the 20 MiB target and 25 MiB Pages Free asset limit.
Expanded JSON stays below 128 MiB. It uses the unchanged version 1 recording
format and standard gzip (level 9, mtime zero).

`metrics.json` contains sampled DevTools JavaScript heap usage for the host
recording/replay viewer: baseline 21,290,088 bytes,
largest sampled recording heap 112,859,556 bytes,
and replay 86,541,840 bytes. These are sampled host Chrome
renderer allocations, not total browser RSS or guest browser memory. The replay
sample also includes the capture script's base64 File API input.

The source, kernel, package, version, BPF feature, VM resource and QEMU manifests
identify the capture environment. Both browsers are installed in this guest;
Chrome is Google's stable package and Firefox is the official Mozilla 158.0
archive verified with release SHA512SUMS. The package manifest identifies drift
from unpinned downloads. Regenerate with `BROWSER=chrome` using
[the VM and capture instructions](../../RECORDING.md#dedicated-ubuntu-2604-browser-recording-vm).
Firefox children can use names such as `Isolated Web Co`; capture validation
checks executable command lines as well as process names.

Publication review covers all command lines, users, mapped/file paths and IP
addresses. The dedicated guest contains OS service accounts and a disposable
`demo` account, with no host home directory, profile, login, credentials or user
documents. Browser profile identifiers, IPC UUIDs and field-trial handles are
created inside that guest. Recordings contain paths and I/O counts, not profile
file contents. 33 recorded IPv4/IPv6 addresses are consistently replaced
with documentation addresses in `192.0.2.0/24` and `2001:db8::/32` across
1866 occurrences. Remaining addresses are unspecified, loopback or QEMU's
private network. No other fields are edited; process/resource identities,
activity counts and event order remain intact.

The public viewer offers Chrome and Firefox only. Synthetic data is confined to
a private test fixture. Schema, activity and backend-free browser checks validate
the published recordings.
