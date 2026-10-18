#!/bin/bash
# Optional QEMU tool environment; the VM itself still uses direct QEMU/KVM.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
state=${VM_DIR:-/tmp/procinsh-recording-vm}
mkdir -p "$state"
state=$(cd "$state" && pwd)
docker build --network host -t procinsh-recording-qemu -f "$root/scripts/recording-vm/Dockerfile" "$root/scripts/recording-vm"
exec docker run --rm --init --network host --device /dev/kvm \
  --user "$(id -u):$(id -g)" --group-add "$(stat -c %g /dev/kvm)" \
  --mount "type=bind,src=$root,dst=/source,readonly" \
  --mount "type=bind,src=$state,dst=$state" \
  -e "VM_DIR=$state" -e "HOST_PORT=${HOST_PORT:-10090}" \
  -e "VM_CPUS=${VM_CPUS:-4}" -e "VM_MEMORY_MIB=${VM_MEMORY_MIB:-8192}" \
  -e "VM_DISK_GIB=${VM_DISK_GIB:-40}" \
  -e "PROCINSH_REF=${PROCINSH_REF:-$(git -C "$root" rev-parse HEAD)}" \
  -e "IMAGE_BASE=${IMAGE_BASE:-https://cloud-images.ubuntu.com/releases/26.04/release-20260927}" \
  procinsh-recording-qemu "$@"
