FROM ubuntu:26.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    qemu-system-x86 qemu-utils cloud-image-utils curl ca-certificates git python3 util-linux \
    && rm -rf /var/lib/apt/lists/*
ENTRYPOINT ["bash", "/source/scripts/recording-vm/run.sh"]
