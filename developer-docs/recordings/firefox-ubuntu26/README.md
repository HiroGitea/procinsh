# Firefox on Ubuntu 26.04

`web/replay-public/recordings/firefox-ubuntu26.json.gz` is a real SPACE recording
of Mozilla Firefox 158.0 from a dedicated Ubuntu 26.04 QEMU/KVM guest. The existing
host-browser Record / Stop & Download controls capture it over direct SLIRP
forwarding, with port 10090 on both the host and guest.

| Observation | Value |
| --- | --- |
| Recorded UTC | 2026-10-18T07:54:19.523Z |
| Duration | 15.7132 seconds |
| Workload | 8 local tabs, 10-second reloads, workers, loopback fetches and IndexedDB writes |
| Guest | Ubuntu 26.04, 4 vCPUs, 8192 MiB RAM, 40 GiB QCOW2 |
| Initial Firefox processes | 18 |
| Processes across snapshots | 164 |
| Snapshot / activity events | 13 / 15 |
| Firefox CPU / IPC / file samples | 144 / 1140 / 335 |
| Original JSON / gzip bytes | 58,329,620 / 2,762,013 |
| Published JSON / gzip bytes | 58,328,042 / 2,776,465 |
| Published gzip SHA256 | `70849116bde02b5191223cba7d24df6e024bedab615ab8b0967a4000abf1d56b` |

The published gzip is below the 20 MiB target and the 25 MiB Cloudflare Pages
Free asset limit. Expanded JSON stays below 128 MiB. It uses the unchanged
version 1 recording format and standard gzip (level 9, mtime zero).

`metrics.json` contains sampled DevTools JavaScript heap usage for the host
recording/replay viewer: baseline 27,196,312 bytes,
largest sampled recording heap 108,404,756 bytes,
and replay 93,269,068 bytes. These measurements cover
the host Chrome viewer, not the Firefox workload or total browser RSS. The replay
sample also includes the capture script's base64 File API input.

The version, kernel, package, BPF feature, VM resource and QEMU manifests in this
directory identify the recording environment. Firefox is the fixed official
Mozilla tarball, verified with its release SHA512SUMS. Regenerate with
`BROWSER=firefox` using [the VM and capture instructions](../../RECORDING.md#dedicated-ubuntu-2604-browser-recording-vm).
Firefox children can use names such as `Isolated Web Co` and `Socket Process`;
the capture checks their executable command lines as well as process names.

Publication review includes all command lines, users, mapped/file paths and IP
addresses. The guest has only OS service accounts and the disposable `demo`
account, with no host home directory, browser profile, account login or user
documents. Firefox IPC UUIDs and profile identifiers are generated inside the
isolated VM and are not authentication credentials. Recordings contain paths and
I/O counts, not the contents of profile or crash-report files. Ten external IPv4
addresses are consistently replaced with `192.0.2.1` through `192.0.2.10` across
356 occurrences. Remaining addresses are unspecified, loopback or
QEMU's private network. No other fields are edited; process/resource identities,
counts and event order remain intact.

The bundled viewer offers Chrome and Firefox recordings only. Synthetic data
is confined to a test fixture and is absent from both public build outputs.
