# Firefox on Ubuntu 26.04

`web/replay-public/recordings/firefox-ubuntu26.json.gz` is a real SPACE
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
| Recorded UTC | 2026-10-18T09:01:15.134Z |
| Duration | 15.3804 seconds |
| Guest | Ubuntu 26.04, 4 vCPUs, 8192 MiB RAM, 40 GiB QCOW2 |
| Initial Firefox processes | 18 |
| Processes across snapshots | 162 |
| Snapshot / activity events | 13 / 15 |
| Firefox CPU / IPC / file samples | 153 / 1193 / 563 |
| Original JSON / gzip bytes | 63,130,582 / 2,946,866 |
| Published JSON / gzip bytes | 63,128,862 / 2,965,213 |
| Published gzip SHA256 | `69b697764baab040c333fa5fe7c7d702afe50a569df8075a9579c0a4a48212bd` |

The published gzip is below the 20 MiB target and 25 MiB Pages Free asset limit.
Expanded JSON stays below 128 MiB. It uses the unchanged version 1 recording
format and standard gzip (level 9, mtime zero).

`metrics.json` contains sampled DevTools JavaScript heap usage for the host
recording/replay viewer: baseline 14,311,072 bytes,
largest sampled recording heap 100,661,880 bytes,
and replay 89,087,740 bytes. These are sampled host Chrome
renderer allocations, not total browser RSS or guest browser memory. The replay
sample also includes the capture script's base64 File API input.

The source, kernel, package, version, BPF feature, VM resource and QEMU manifests
identify the capture environment. Both browsers are installed in this guest;
Chrome is Google's stable package and Firefox is the official Mozilla 158.0
archive verified with release SHA512SUMS. The package manifest identifies drift
from unpinned downloads. Regenerate with `BROWSER=firefox` using
[the VM and capture instructions](../../RECORDING.md#dedicated-ubuntu-2604-browser-recording-vm).
Firefox children can use names such as `Isolated Web Co`; capture validation
checks executable command lines as well as process names.

Publication review covers all command lines, users, mapped/file paths and IP
addresses. The dedicated guest contains OS service accounts and a disposable
`demo` account, with no host home directory, profile, login, credentials or user
documents. Browser profile identifiers, IPC UUIDs and field-trial handles are
created inside that guest. Recordings contain paths and I/O counts, not profile
file contents. 10 recorded IPv4/IPv6 addresses are consistently replaced
with documentation addresses in `192.0.2.0/24` and `2001:db8::/32` across
344 occurrences. Remaining addresses are unspecified, loopback or QEMU's
private network. No other fields are edited; process/resource identities,
activity counts and event order remain intact.

The public viewer offers Chrome and Firefox only. Synthetic data is confined to
a private test fixture. Schema, activity and backend-free browser checks validate
the published recordings.
