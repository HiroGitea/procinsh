#!/bin/bash
# Build a real kernel in the disposable guest, with separate source and output.
set -euo pipefail
source_dir=/home/demo/linux
build_dir=/home/demo/linux-build
cpus=$(nproc)
jobs=$((cpus > 2 ? cpus - 2 : 1))
kernel_flags="-Wno-error=default-const-init-field-unsafe -Wno-error=default-const-init-var-unsafe"
export CCACHE_DIR=/home/demo/.cache/kernel-ccache
case ${1:-} in
  prepare)
    git clone --depth 1 --branch v6.14 https://github.com/torvalds/linux "$source_dir"
    mkdir -p "$build_dir"
    make -C "$source_dir" O="$build_dir" CC="ccache clang" LOCALVERSION=-procinsh-demo defconfig
    make -C "$source_dir" O="$build_dir" CC="ccache clang" LOCALVERSION=-procinsh-demo kvm_guest.config
    {
      git -C "$source_dir" rev-parse HEAD
      clang --version
      printf 'jobs=%s\nCC=ccache clang\nLOCALVERSION=-procinsh-demo\nKCFLAGS=%s\n' "$jobs" "$kernel_flags"
      sha256sum "$build_dir/.config"
    } > /home/demo/kernel-build-info.txt
    ;;
  build)
    test -f "$build_dir/.config"
    make -C "$source_dir" O="$build_dir" CC="ccache clang" LOCALVERSION=-procinsh-demo KCFLAGS="$kernel_flags" -j "$jobs"
    "$source_dir/scripts/clang-tools/gen_compile_commands.py" --directory "$build_dir"
    ;;
  *) echo "Usage: $0 {prepare|build}" >&2; exit 2 ;;
esac
