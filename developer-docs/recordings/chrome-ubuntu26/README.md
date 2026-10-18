# Chrome on Ubuntu 26.04

`web/replay-public/recordings/chrome-ubuntu26.json.gz` is a real SPACE recording
captured through the existing browser Record / Stop & Download controls over
QEMU SLIRP forwarding (`127.0.0.1:10090` to guest port 9090).

| Observation | Value |
| --- | --- |
| Recorded UTC | 2026-10-18 05:52:00 |
| Duration | 15.3224 seconds |
| Workload | 8 local Chrome tabs, 10-second reloads, workers, loopback fetches, IndexedDB writes |
| Guest | Ubuntu 26.04, 4 vCPUs, 8192 MiB RAM, 40 GiB QCOW2 |
| Chrome processes in initial snapshot | 19 |
| Processes across snapshots | 165 |
| Snapshot / activity events | 12 / 15 |
| Chrome CPU / IPC / file activity samples | 169 / 1719 / 758 |
| Original JSON / gzip bytes | 57,352,240 / 2,820,455 |
| Published JSON / gzip bytes | 57,352,072 / 2,837,300 |
| Published gzip SHA256 | `32b2dcc06309d11688099f3237505ff6e0587447aa6f0aa34445065f62b7e96c` |

The gzip is below the 20 MiB target and 25 MiB Pages Free limit. Expanded JSON
is below the 128 MiB recording/replay limit. The original gzip was generated with
Node zlib level 9; the published gzip uses Python gzip level 9 with `mtime=0`.
Both are standard gzip streams. The source recording format remains version 1.

`metrics.json` retains browser DevTools `Runtime.getHeapUsage` samples: the live
baseline used 16,759,300 bytes of JavaScript heap, the largest sampled recording
heap was 108,280,752 bytes, and replay used 85,556,496 bytes. Replay backing storage
was 63,837,045 bytes. These are sampled JavaScript/renderer allocations, not total
browser RSS or a guarantee of peak allocation. The capture script retains its
base64 File API input during the replay measurement, so that sample includes test
input storage. The measured duration fits the current in-memory recorder without
changing the format or increasing its limit.

`source.json`, `kernel.txt`, `versions.txt`, `packages.txt`, `bpf-features.txt`,
`resources.txt` and `qemu-version.txt` record the environment. Regeneration uses
[the VM and capture commands](../../RECORDING.md#dedicated-ubuntu-2604-browser-recording-vm).
The full package manifest identifies unpinned package drift; matching it requires
retaining the guest disk or supplying package archives for the listed versions.

The dedicated VM contains no host home directory, browser profile, account login,
credentials or user documents. Publication review covers all recorded command
lines, usernames, memory-map/file paths and IP strings. Users are `demo` and OS
service accounts; paths identify distribution packages, Chrome, procinsh and the
disposable Chrome profile. Chrome field-trial IDs and pseudonymization salt handles
are generated within that disposable profile and are not authentication tokens.
One external IPv4 address is consistently replaced with the documentation address
`192.0.2.1` everywhere in the serialized recording (28 occurrences); process and
resource identities, activity counts and event order are preserved. Remaining
IPv4 addresses are loopback, unspecified, or QEMU's private `10.0.2.0/24` network.
No other fields are edited. Schema, playback and asset-size checks validate the
published gzip, including backend-free playback under a deployment prefix.
