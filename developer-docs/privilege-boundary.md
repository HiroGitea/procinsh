# Acquisition and analysis boundary

The privileged helper owns protected reads and kernel resources. The HTTP process
clears capabilities and interprets acquired values before exposing the existing
HTTP API. No file descriptor crosses IPC.

| Helper acquisition | Main interpretation |
| --- | --- |
| Protected procfs reads and process/FD identity checks | Process and thread rates, histories, usernames, environment/auxv formatting |
| Ptrace/perf registers, stack bytes, instruction bytes and mapping metadata | Register classification, framehop unwinding and iced-x86 disassembly |
| Open a currently executable mapped file, pin and verify its device/inode, read bounded bytes and recheck metadata | ELF/DWARF parsing, decompression, symbol/source lookup and caching |
| FD ownership, socket metadata, namespace diagnostics and IPoIB neighbor observations | Peer/shared-holder relationships and destination enrichment |
| BPF loading, attachment, bounded ring draining and map reads | File/IPC aggregation, CPU counter deltas and signal identity matching |
| Character-device metadata from protected per-process FD scans | Unprivileged sysfs/procfs hardware topology, paths and device-handle grouping |
| Observation sessions, acquisition cancellation and kernel resource cleanup | Reverse DNS workers and caches; response formatting |

IPC replies are limited to 1 GiB, requests to 1 KiB. Mapped ELF requests contain a
process identity and mapping start address. The IPC codec uses native-endian
numbers, explicit enum tags and length-prefixed UTF-8 strings and collections.
ELF bytes are copied as one raw byte block. Acquisition snapshots retain typed
numeric fields until main constructs the HTTP JSON representation. The codec
copies values explicitly and never copies Rust padding or pointers. Each payload
has a protocol version marker; peers must use the same version and architecture.
Main supplies known file versions so unchanged files are not transferred again,
including files rejected by the parser. The helper selects a current
executable mapping itself; it accepts no caller-selected path or memory-read
range. ELF acquisition retains the 128 MiB file limit and validates the opened
regular file before and after reading. Main retains parser expansion and cache
limits and verifies the transferred file identity against the captured mapping.

Low-level ABI decoding, mapping parsing used to validate capture bounds, numeric
auxv parsing needed to read selected strings, queue limits, and process/file
identity checks stay with acquisition. These establish what can be acquired and
bound the helper's work independently of main.

Process rates and histories and BPF CPU deltas belong to an observation session.
Mapping snapshots are retained by epoch and shared across thread samples; each
referenced epoch is sent once per observation so old samples keep their original
address-space context. Raw counters, sampled stacks, ELF bytes and other internal IPC metadata are
removed before HTTP serialization. Analysis failures retain raw-frame or
acquisition-error fallbacks where the API supports them.
