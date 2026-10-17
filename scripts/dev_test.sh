#!/usr/bin/env bash
set -euo pipefail

# Backward-compatible entry point for running a named Rust unit test.
exec bash "$(dirname "$0")/docker_test.sh" unit "$@"
