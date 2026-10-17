#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
mode=all
if [[ ${1:-} == unit ]]; then
    mode=unit
    shift
fi

# Never require sudo/setcap on the host. In rootless Docker, host-kernel
# capabilities are generally unavailable, so the mandatory tests will fail.
docker build -f "$repo/tests/Dockerfile" -t procinsh-kernel-tests:local "$repo"
docker run --rm \
    --pid=host \
    --cap-add=BPF \
    --cap-add=PERFMON \
    --cap-add=SYS_PTRACE \
    --cap-add=DAC_READ_SEARCH \
    --cap-add=SYS_ADMIN \
    --ulimit memlock=-1:-1 \
    --mount "type=bind,src=$repo,dst=/source,readonly" \
    --mount type=bind,src=/sys/kernel/btf,dst=/sys/kernel/btf,readonly \
    --mount type=volume,src=procinsh-kernel-test-target,dst=/work/target \
    --workdir /work \
    procinsh-kernel-tests:local bash -s -- "$mode" "$@" <<'IN_CONTAINER'
set -euo pipefail

mode=$1
shift

# Work from a disposable copy: container-root must never create root-owned
# files in the developer's checkout, including target/ and web/dist/.
(cd /source && tar --exclude='./target' --exclude='./web/node_modules' \
    --exclude='./web/dist' -cf - .) | tar -C /work -xf -

test -r /sys/kernel/btf/vmlinux
npm --prefix web ci
npm --prefix web run build
sh tests/targets/build.sh
cargo build --locked

if [[ $mode == unit ]]; then
    cargo test --locked --bin procinsh "$@"
    exit
fi

PROCINSH_BINARY=./target/debug/procinsh cargo test --locked

./target/debug/procinsh --listen 127.0.0.1:9090 &
server=$!
trap 'kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true' EXIT
ready=0
for _ in {1..40}; do
    if curl -fsS http://127.0.0.1:9090/api/config >/dev/null 2>&1; then
        ready=1
        break
    fi
    kill -0 "$server"
    sleep 0.25
done
if (( !ready )); then
    echo "procinsh server did not become ready" >&2
    exit 1
fi

python3 tests/space-live.py http://127.0.0.1:9090
python3 tests/space-files-live.py http://127.0.0.1:9090
python3 tests/space-signals-live.py http://127.0.0.1:9090
IN_CONTAINER
