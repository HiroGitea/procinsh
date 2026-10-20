#!/bin/bash
# Build the running guest's Ubuntu kernel source with separate source and output.
set -euo pipefail
source_dir=/home/demo/linux
archive_dir=/home/demo/kernel-source
build_dir=/home/demo/linux-build
guest_release=$(uname -r)
source_package=$(dpkg-query -W -f='${source:Package}' "linux-modules-$guest_release")
source_version=$(dpkg-query -W -f='${source:Version}' "linux-modules-$guest_release")
cpus=$(nproc)
jobs=$((cpus > 2 ? cpus - 2 : 1))
case ${1:-} in
  prepare)
    mkdir -p "$archive_dir"
    cd "$archive_dir"
    apt-get source --download-only --only-source "$source_package=$source_version"
    dpkg-source -x "${source_package}_${source_version#*:}.dsc" "$source_dir"
    ;;
  build) test -f "$build_dir/.config" ;;
  *) echo "Usage: $0 {prepare|build}" >&2; exit 2 ;;
esac
kernel_version=$(make -s -C "$source_dir" kernelversion)
guest_base=${guest_release%%-*}
[[ ${kernel_version%.*} == "${guest_base%.*}" ]] || {
  echo "Source kernel version $kernel_version does not match guest series $guest_base" >&2
  exit 1
}
changelog_version=$(dpkg-parsechangelog -l "$source_dir/debian.master/changelog" -SVersion)
[[ $changelog_version == "$source_version" ]] || {
  echo "Source package version $changelog_version does not match installed $source_version" >&2
  exit 1
}
kernel_make() {
  # Ubuntu's packaging sets KERNELRELEASE independently of stable patch level.
  make --no-print-directory -C "$source_dir" O="$build_dir" CC=clang KERNELRELEASE="$guest_release" "$@"
}
if [[ $1 == prepare ]]; then
  mkdir -p "$build_dir"
  kernel_make defconfig
  kernel_make kvm_guest.config
fi
built_release=$(kernel_make -s kernelrelease)
[[ $built_release == "$guest_release" ]] || {
  echo "Build kernel release $built_release does not match guest $guest_release" >&2
  exit 1
}
if [[ $1 == prepare ]]; then
  {
    printf 'guest_release=%s\nsource_package=%s\nsource_version=%s\n' "$guest_release" "$source_package" "$source_version"
    printf 'kernelversion=%s\nkernelrelease=%s\njobs=%s\nCC=clang\n' "$kernel_version" "$built_release" "$jobs"
    clang --version
    sha256sum "$archive_dir"/*.dsc "$build_dir/.config"
  } > /home/demo/kernel-build-info.txt
else
  kernel_make -j "$jobs"
  "$source_dir/scripts/clang-tools/gen_compile_commands.py" --directory "$build_dir"
fi
