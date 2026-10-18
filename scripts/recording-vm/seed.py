#!/usr/bin/env python3
"""Generate a NoCloud seed without copying host credentials into the guest."""
import json
import os
from pathlib import Path
import re
import sys

state, revision = Path(sys.argv[1]), sys.argv[2]
port = int(sys.argv[3])
if not 1 <= port <= 65535:
    raise SystemExit("HOST_PORT must be between 1 and 65535")
if not re.fullmatch(r"[0-9a-f]{40}", revision):
    raise SystemExit("PROCINSH_REF must be a full, published commit SHA")
browser = os.environ.get("BROWSER", "chrome")
if browser not in ("chrome", "firefox"):
    raise SystemExit("BROWSER must be chrome or firefox")
setup = Path(__file__).with_name("setup.sh").read_text()
config = {
    "hostname": "space-demo",
    "manage_etc_hosts": True,
    "ssh_pwauth": False,
    "disable_root": True,
    "users": [{"name": "demo", "lock_passwd": True, "shell": "/bin/bash", "groups": "sudo", "sudo": "ALL=(ALL) NOPASSWD:ALL"}],
    "package_update": True,
    "packages": ["git", "curl", "ca-certificates", "build-essential", "clang", "llvm",
                 "pkg-config", "libelf-dev", "zlib1g-dev", "bpftool", "nodejs", "npm", "python3"],
    "write_files": [
        {"path": "/opt/setup-recording.sh", "permissions": "0755", "content": setup},
    ],
    "runcmd": [["bash", "/opt/setup-recording.sh", revision, str(port), browser]],
}
(state/"user-data").write_text("#cloud-config\n" + json.dumps(config, indent=2) + "\n")
(state/"meta-data").write_text("instance-id: space-demo-" + revision + "\nlocal-hostname: space-demo\n")
(state/"http-port").write_text(str(port) + "\n")
