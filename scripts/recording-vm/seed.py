#!/usr/bin/env python3
"""Generate a NoCloud seed without copying host credentials into the guest."""
import json
from pathlib import Path
import re
import sys

state, revision, image_url = Path(sys.argv[1]), sys.argv[2], sys.argv[3]
if not re.fullmatch(r"[0-9a-f]{40}", revision):
    raise SystemExit("PROCINSH_REF must be a full, published commit SHA")
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
        {"path": "/opt/recording-source.json", "content": json.dumps({
            "revision": revision, "image_url": image_url,
            "image_sha256": next(line.split()[0] for line in (state/"SHA256SUMS").read_text().splitlines()
                                 if line.split()[-1].lstrip('*') == image_url.rsplit('/', 1)[1]),
        }, indent=2)},
    ],
    "runcmd": [["bash", "/opt/setup-recording.sh", revision]],
}
(state/"user-data").write_text("#cloud-config\n" + json.dumps(config, indent=2) + "\n")
(state/"meta-data").write_text("instance-id: space-demo-" + revision + "\nlocal-hostname: space-demo\n")
