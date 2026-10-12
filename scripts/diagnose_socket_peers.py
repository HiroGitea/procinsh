#!/usr/bin/env python3
"""Inspect unresolved procinsh sockets using temporary pidfd_getfd duplicates.

Linux x86-64/aarch64, Python 3.9+, and ptrace access to each target are required.
No process is attached/stopped and no socket data, flags, or SO_ERROR are touched.
Output is JSON Lines; progress and permission guidance go to stderr.
"""

import argparse
import collections
import ctypes
import errno
import ipaddress
import json
import os
import platform
import socket
import stat
import struct
import sys
import time
import urllib.request


LIBC = ctypes.CDLL(None, use_errno=True)
LIBC.syscall.restype = ctypes.c_long
LIBC.getsockname.argtypes = [ctypes.c_int, ctypes.c_void_p,
                             ctypes.POINTER(ctypes.c_uint)]
LIBC.getpeername.argtypes = LIBC.getsockname.argtypes
LIBC.getsockopt.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int,
                           ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint)]


def error_info(error):
    return {"errno": error.errno,
            "code": errno.errorcode.get(error.errno, "UNKNOWN"),
            "message": str(error)}


def check_call(result):
    if result < 0:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code))
    return result


def start_ticks(pid):
    with open(f"/proc/{pid}/stat") as source:
        # comm can contain spaces and parentheses; field 22 follows the last ')'.
        return int(source.read().rsplit(")", 1)[1].split()[19])


def decode_address(raw):
    """Keep raw sockaddr bytes alongside readable, family-specific fields."""
    result = {"raw_hex": raw.hex()}
    if len(raw) < 2:
        return result
    family = struct.unpack_from("=H", raw)[0]
    result["family"] = family
    if family == socket.AF_INET and len(raw) >= 16:
        result.update(ip=str(ipaddress.ip_address(raw[4:8])),
                      port=struct.unpack_from("!H", raw, 2)[0])
    elif family == socket.AF_INET6 and len(raw) >= 28:
        result.update(ip=str(ipaddress.ip_address(raw[8:24])),
                      port=struct.unpack_from("!H", raw, 2)[0],
                      scope_id=struct.unpack_from("=I", raw, 24)[0])
    elif family == socket.AF_UNIX:
        path = raw[2:]
        if not path:
            result["kind"] = "unnamed"
        elif path[0] == 0:
            result.update(kind="abstract", name_hex=path[1:].hex(),
                          name=os.fsdecode(path[1:]))
        else:
            result.update(kind="pathname", path=os.fsdecode(path.split(b"\0", 1)[0]))
    elif family == socket.AF_NETLINK and len(raw) >= 12:
        port_id, groups = struct.unpack_from("=II", raw, 4)
        # A Netlink port ID is not necessarily a process PID.
        result.update(port_id=port_id, groups=groups,
                      endpoint="kernel" if port_id == 0 else "userspace_port")
    elif family == socket.AF_PACKET and len(raw) >= 20:
        result.update(protocol=struct.unpack_from("!H", raw, 2)[0],
                      interface_index=struct.unpack_from("=i", raw, 4)[0])
    elif family == 40 and len(raw) >= 16:  # AF_VSOCK
        port, cid = struct.unpack_from("=II", raw, 4)
        result.update(port=port, cid=cid)
    return result


def address(fd, function):
    storage = (ctypes.c_uint64 * 32)()
    length = ctypes.c_uint(ctypes.sizeof(storage))
    try:
        check_call(function(fd, storage, ctypes.byref(length)))
    except OSError as error:
        return {"error": error_info(error)}
    result = decode_address(bytes(storage)[:length.value])
    result["length"] = length.value
    if length.value > ctypes.sizeof(storage):
        result["truncated"] = True
    return result


def option(fd, name, size=4):
    storage = ctypes.create_string_buffer(size)
    length = ctypes.c_uint(size)
    check_call(LIBC.getsockopt(fd, socket.SOL_SOCKET, name,
                              storage, ctypes.byref(length)))
    if length.value != size:
        raise OSError(errno.EINVAL, "unexpected socket option length")
    return storage.raw


def inspect_fd(fd):
    result = {}
    for name, code in (("domain", socket.SO_DOMAIN), ("type", socket.SO_TYPE),
                       ("protocol", socket.SO_PROTOCOL),
                       ("accepting", socket.SO_ACCEPTCONN)):
        try:
            result[name] = struct.unpack("=i", option(fd, code))[0]
        except OSError as error:
            result[name] = {"error": error_info(error)}
    result["local"] = address(fd, LIBC.getsockname)
    result["peer"] = address(fd, LIBC.getpeername)
    if result.get("domain") == socket.AF_UNIX:
        try:
            pid, uid, gid = struct.unpack("=iII", option(fd, socket.SO_PEERCRED, 12))
            result["peer_credentials"] = {"pid": pid, "uid": uid, "gid": gid}
            result["peer_credentials"]["note"] = (
                "credentials at connect/listen/socketpair time; not current FD ownership")
        except OSError as error:
            result["peer_credentials"] = {"error": error_info(error)}
    peer = result["peer"]
    if result.get("accepting") == 1:
        result["classification"] = "listener"
    elif "error" in peer:
        result["classification"] = ("unconnected" if peer["error"]["errno"] == errno.ENOTCONN
                                    else "peer_query_failed")
    elif result.get("domain") == socket.AF_NETLINK:
        result["classification"] = "netlink_default_peer"
        result["note"] = "getpeername reports the default destination, not sendmsg destinations"
    elif peer.get("kind") == "unnamed":
        result["classification"] = "unnamed_unix_peer"
    else:
        result["classification"] = "peer_address_found"
    return result


def inspect_target(target):
    pid, fd = target["pid"], target["fd"]
    expected_ticks = target["start_time_ticks"]
    if start_ticks(pid) != expected_ticks:
        raise OSError(errno.ESTALE, "process identity changed")
    pidfd = os.pidfd_open(pid)
    try:
        # pidfd_getfd is syscall 438 on Linux x86-64 and aarch64.
        duplicate = check_call(LIBC.syscall(ctypes.c_long(438), ctypes.c_int(pidfd),
                                           ctypes.c_int(fd), ctypes.c_uint(0)))
        try:
            metadata = os.fstat(duplicate)
            if (not stat.S_ISSOCK(metadata.st_mode)
                    or metadata.st_ino != target["inode"]
                    or metadata.st_dev != target["device"]):
                raise OSError(errno.ESTALE, "socket FD identity changed")
            result = inspect_fd(duplicate)
            if start_ticks(pid) != expected_ticks:
                raise OSError(errno.ESTALE, "process identity changed")
            return result
        finally:
            os.close(duplicate)
    finally:
        os.close(pidfd)


def read_snapshot(url, timeout):
    deadline = time.monotonic() + timeout
    request = urllib.request.Request(url.rstrip("/") + "/api/system/events",
                                     headers={"Accept": "text/event-stream"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        payload = []
        for line in response:
            if time.monotonic() > deadline:
                raise TimeoutError("no populated system snapshot before timeout")
            if line.startswith(b"data:"):
                payload.append(line[5:].strip())
            elif not line.strip() and payload:
                data = json.loads(b"\n".join(payload))
                payload = []
                if data.get("processes"):
                    return data
    raise RuntimeError("system event stream ended without a populated snapshot")


def snapshot_targets(snapshot):
    names = {p["identity"]["pid"]: p["name"] for p in snapshot["processes"]}
    seen = set()
    for relation in snapshot["fd_relations"]:
        endpoint = relation["endpoint"]
        info = relation.get("socket") or {}
        if (endpoint["kind"] != "socket" or relation.get("peer") is not None
                or info.get("network_peer")):
            continue
        pid, fd = endpoint["process_id"]["pid"], endpoint["fd"]
        if (pid, fd) in seen:
            continue
        seen.add((pid, fd))
        resource = endpoint["resource"]
        device = resource["device"]
        yield {"pid": pid, "fd": fd, "name": names.get(pid),
               "start_time_ticks": endpoint["process_id"]["start_time_ticks"],
               "inode": int(resource["inode"]),
               "device": os.makedev(device["major"], device["minor"]),
               "procinsh_label": relation["label"], "procinsh_socket": info}


def process_targets(pid, requested_fd):
    ticks = start_ticks(pid)
    with open(f"/proc/{pid}/comm") as source:
        name = source.read().strip()
    fds = [str(requested_fd)] if requested_fd is not None else os.listdir(f"/proc/{pid}/fd")
    for fd in sorted(fds, key=int):
        try:
            metadata = os.stat(f"/proc/{pid}/fd/{fd}")
        except FileNotFoundError:
            if requested_fd is not None:
                raise
            continue
        if stat.S_ISSOCK(metadata.st_mode):
            yield {"pid": pid, "fd": int(fd), "name": name, "start_time_ticks": ticks,
                   "inode": metadata.st_ino, "device": metadata.st_dev}
        elif requested_fd is not None:
            raise OSError(errno.ENOTSOCK, "requested FD is not a socket")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--url", help="procinsh base URL; inspect sockets without a known destination")
    source.add_argument("--snapshot", help="saved system snapshot JSON")
    source.add_argument("--pid", type=int, help="inspect all sockets owned by this PID")
    parser.add_argument("--fd", type=int, help="inspect one FD (requires --pid)")
    parser.add_argument("--timeout", type=float, default=20, help="SSE timeout in seconds")
    args = parser.parse_args()
    if args.fd is not None and args.pid is None:
        parser.error("--fd requires --pid")
    if (args.pid is not None and args.pid <= 0) or (args.fd is not None and args.fd < 0):
        parser.error("PID must be positive and FD nonnegative")
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    if sys.platform != "linux" or platform.machine() not in ("x86_64", "aarch64"):
        parser.error("pidfd_getfd syscall mapping supports Linux x86-64/aarch64 only")
    try:
        if args.pid is not None:
            targets = list(process_targets(args.pid, args.fd))
        else:
            if args.url:
                snapshot = read_snapshot(args.url, args.timeout)
            else:
                with open(args.snapshot) as source_file:
                    snapshot = json.load(source_file)
            targets = list(snapshot_targets(snapshot))
        counts = collections.Counter()
        denied = 0
        for target in targets:
            try:
                result = inspect_target(target)
                counts[result["classification"]] += 1
            except OSError as error:
                result = {"error": error_info(error)}
                counts["inspection_failed"] += 1
                denied += error.errno in (errno.EPERM, errno.EACCES)
            print(json.dumps({**target, **result}, ensure_ascii=True), flush=True)
        print(json.dumps({"sockets": len(targets), "results": counts,
                          "permission_denied": denied}), file=sys.stderr)
        if denied:
            print("ptrace access is required for pidfd_getfd. Run this script with sudo "
                  "on the host; container seccomp/LSM policies may still deny access.",
                  file=sys.stderr)
        return 1 if targets and counts["inspection_failed"] == len(targets) else 0
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        print(f"diagnosis failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
