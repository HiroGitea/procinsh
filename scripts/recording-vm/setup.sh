#!/bin/bash
set -euo pipefail
exec > >(tee /var/log/recording-setup.log) 2>&1
test -r /sys/kernel/btf/vmlinux
git clone https://github.com/akawashiro/procinsh.git /opt/procinsh
cd /opt/procinsh
git checkout --detach "$1"
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup.sh
RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo sh /tmp/rustup.sh -y --default-toolchain none
export RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
npm --prefix web ci
npm --prefix web run build
cargo build --locked
curl -fL https://dl.google.com/linux/direct/google-chrome-stable_current_amd64.deb -o /tmp/chrome.deb
apt-get install -y /tmp/chrome.deb
mkdir -p /opt/recording-metadata
cp /opt/recording-source.json /opt/recording-metadata/source.json
uname -a >/opt/recording-metadata/kernel.txt
dpkg-query -W >/opt/recording-metadata/packages.txt
{ rustc --version; cargo --version; node --version; npm --version; google-chrome --version; bpftool version; git rev-parse HEAD; } >/opt/recording-metadata/versions.txt
bpftool feature probe kernel >/opt/recording-metadata/bpf-features.txt
chown -R demo:demo /opt/recording-metadata
cat >/etc/systemd/system/space-demo.service <<'UNIT'
[Unit]
Description=Dedicated SPACE recording collector
After=network.target
[Service]
WorkingDirectory=/opt/procinsh
ExecStart=/bin/bash /opt/procinsh/scripts/recording-vm/start-collector.sh
Restart=on-failure
[Install]
WantedBy=multi-user.target
UNIT
cat >/etc/systemd/system/chrome-demo.service <<'UNIT'
[Unit]
Description=Local Chrome workload for SPACE recordings
After=network.target
[Service]
User=demo
WorkingDirectory=/opt/procinsh
Environment=TAB_COUNT=8 INTERVAL_SECONDS=10
ExecStart=/usr/bin/node scripts/chrome_workload.mjs
Restart=on-failure
[Install]
WantedBy=multi-user.target
UNIT
# The console is local to QEMU; there is no SSH listener or password login.
mkdir -p /etc/systemd/system/serial-getty@ttyS0.service.d
printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin demo --noclear %%I $TERM\n' >/etc/systemd/system/serial-getty@ttyS0.service.d/autologin.conf
systemctl daemon-reload
systemctl enable --now space-demo chrome-demo
echo 'SPACE recording ready on guest port 9090'
