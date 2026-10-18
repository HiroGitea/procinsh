#!/bin/bash
set -euo pipefail
# QEMU passes the current hostfwd port at every boot, including VM restarts.
modprobe qemu_fw_cfg
port=$(cat /sys/firmware/qemu_fw_cfg/by_name/opt/procinsh/host-port/raw)
[[ $port =~ ^[0-9]+$ && $port -ge 1 && $port -le 65535 ]] || {
  echo 'Missing or invalid QEMU HTTP forwarding port' >&2; exit 1;
}
exec /opt/procinsh/scripts/dev_run.sh --listen 0.0.0.0:9090 \
  --allow-non-loopback --http-host-port "$port"
