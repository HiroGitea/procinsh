// Shared formatting. Values remain structured in models and API contracts.
export namespace Display {
  export function socketType(
    type: import("./api-types.js").SocketType,
  ): string {
    return type.kind === "unknown"
      ? `TYPE ${type.code}`
      : type.kind.toUpperCase();
  }
  export function permissions(
    m: import("./api-types.js").MappingPermissions,
  ): string {
    return `${m.readable ? "r" : "-"}${m.writable ? "w" : "-"}${m.executable ? "x" : "-"}${m.private ? "p" : "s"}`;
  }
  export function mapping(m: import("./api-types.js").RegisterMapping): string {
    return `${m.pathname ?? "[anonymous]"} [${permissions(m)}]`;
  }
  export function scheduler(
    p: import("./api-types.js").SchedulerPolicy,
  ): string {
    return p.kind === "unknown" ? `UNKNOWN (${p.code})` : p.kind.toUpperCase();
  }
  export function affinity(
    ranges: import("./api-types.js").CpuRange[] | null,
  ): string {
    return ranges === null
      ? "N/A"
      : ranges
          .map((r) =>
            r.start === r.end ? String(r.start) : `${r.start}-${r.end}`,
          )
          .join(",");
  }
  export function address(
    a: import("./api-types.js").InetAddress | null | undefined,
  ): string {
    return a ? `${a.ip.includes(":") ? `[${a.ip}]` : a.ip}:${a.port}` : "";
  }
  export function protocol(
    p: import("./api-types.js").SocketProtocol | null | undefined,
  ): string {
    if (!p) return "";
    if (p.kind === "unix") {
      return `UNIX ${socketType(p.socket_type)}`;
    }
    if (p.kind === "other") {
      if (p.family === 16) {
        const names: Record<number, string> = {
          0: "ROUTE",
          1: "UNUSED",
          2: "USERSOCK",
          3: "FIREWALL",
          4: "SOCK_DIAG",
          5: "NFLOG",
          6: "XFRM",
          7: "SELINUX",
          8: "ISCSI",
          9: "AUDIT",
          10: "FIB_LOOKUP",
          11: "CONNECTOR",
          12: "NETFILTER",
          13: "IP6_FW",
          14: "DNRTMSG",
          15: "KOBJECT_UEVENT",
          16: "GENERIC",
          18: "SCSITRANSPORT",
          19: "ECRYPTFS",
          20: "RDMA",
          21: "CRYPTO",
          22: "SMC",
        };
        const type =
          p.socket_type?.kind === "unknown"
            ? ` TYPE ${p.socket_type.code}`
            : "";
        return `NETLINK ${names[p.protocol] ?? `PROTOCOL ${p.protocol}`}${type}`;
      }
      if (
        (p.family === 2 || p.family === 10) &&
        p.socket_type?.kind === "raw"
      ) {
        const names: Record<number, string> = {
          1: "ICMP",
          2: "IGMP",
          6: "TCP",
          17: "UDP",
          58: "ICMPV6",
        };
        return `RAW${p.family === 10 ? "6" : ""} ${names[p.protocol] ?? `PROTOCOL ${p.protocol}`}`;
      }
      const families: Record<number, string> = {
        2: "INET",
        10: "INET6",
        17: "PACKET",
        40: "VSOCK",
        31: "BLUETOOTH",
        29: "CAN",
        38: "ALG",
        44: "XDP",
      };
      const type = p.socket_type ? ` ${socketType(p.socket_type)}` : "";
      const protocol =
        (p.family === 17 || p.family === 40) && p.protocol === 0
          ? ""
          : ` PROTOCOL ${p.protocol}`;
      return `${families[p.family] ?? `AF ${p.family}`}${type}${protocol}`;
    }
    return `${p.kind.toUpperCase()}${p.family === "ipv6" ? "6" : ""}`;
  }
  export function state(
    s: import("./api-types.js").SocketState | null | undefined,
  ): string {
    if (!s) return "";
    return s.kind === "unknown_inet" || s.kind === "unknown_unix"
      ? s.code.toString(16).toUpperCase().padStart(2, "0")
      : s.kind.toUpperCase();
  }
  export function access(a: import("./api-types.js").FdAccess): string {
    return a === "unknown" ? "N/A" : a === "read_write" ? "read/write" : a;
  }
}

export const num = (v: number | null | undefined, digits = 1) =>
  v == null
    ? "N/A"
    : v.toLocaleString("en-US", { maximumFractionDigits: digits });
export const percent = (v: number | null | undefined) =>
  v == null ? "N/A" : `${num(v)}%`;
export function bytes(v: number | null | undefined) {
  if (v == null) return "N/A";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${num(v)} ${units[i]}`;
}
export const rate = (v: number | null | undefined) =>
  v == null ? "N/A" : `${num(v)}/s`;
export const byteRate = (v: number | null | undefined) =>
  v == null ? "N/A" : `${bytes(v)}/s`;
