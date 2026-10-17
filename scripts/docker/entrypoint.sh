#!/bin/bash
set -euo pipefail

# All writes, including fixture builds and HMR edits, stay in the container.
tar -C /source --exclude='./target' --exclude='./web/node_modules' \
  --exclude='./web/dist' --exclude='./tests/targets/bin' \
  --exclude='./.venv-format' --exclude='./.aws' --exclude='./.codex' \
  --exclude='./.agents' -cf - . | tar -C /work -xf -
git config --global --add safe.directory /work
test -r /sys/kernel/btf/vmlinux || {
  echo 'The host kernel must expose /sys/kernel/btf/vmlinux (CONFIG_DEBUG_INFO_BTF=y).' >&2
  exit 1
}
bpftool version
npm --prefix web ci
npm --prefix web run build

suite=$1
shift
case "$suite" in
  ordinary)
    cargo build --locked --all-targets
    cargo test --locked
    npm --prefix web test
    python3 tests/listen-policy.py
    npm --prefix web run test:dev
    ;;
  checks)
    bash -n scripts/docker_test.sh scripts/docker/entrypoint.sh
    npm --prefix web run format:check
    cargo fmt --all --check
    python3 scripts/format_c.py --check
    cargo clippy --locked --all-targets -- -D warnings
    RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links' \
      cargo doc --locked --workspace --no-deps --document-private-items
    npm --prefix web run docs
    npm --prefix web run docs:architecture
    python3 scripts/check_web_package.py
    ;;
  kernel)
    cargo test --locked -- --ignored
    cargo build --locked
    sh tests/targets/build.sh
    log=$(mktemp)
    target/debug/procinsh --listen 127.0.0.1:0 >"$log" 2>&1 &
    server=$!
    cleanup() {
      kill "$server" 2>/dev/null || true
      wait "$server" 2>/dev/null || true
      cat "$log"
      rm -f "$log"
    }
    trap cleanup EXIT
    base=$(python3 - "$log" "$server" <<'PY'
import os
from pathlib import Path
import re
import sys
import time
for _ in range(200):
    os.kill(int(sys.argv[2]), 0)
    match = re.search(r'listening on (http://127\.0\.0\.1:\d+)', Path(sys.argv[1]).read_text())
    if match:
        print(match[1])
        break
    time.sleep(0.05)
else:
    raise SystemExit('Live test server did not become ready')
PY
    )
    python3 tests/space-live.py "$base"
    python3 tests/space-files-live.py "$base"
    python3 tests/space-signals-live.py "$base"
    ;;
  browser)
    cargo build --locked
    sh tests/targets/build.sh
    export CHROME=/usr/bin/google-chrome
    npm --prefix web run test:browser
    ;;
  rust) cargo test --locked -- --include-ignored "$@" ;;
  web) npm --prefix web test -- "$@" ;;
  *) echo "Unknown container test suite: $suite" >&2; exit 2 ;;
esac
