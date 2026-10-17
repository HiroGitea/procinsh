# Privileged Linux integration test environment (privileges are supplied by docker run).
FROM node:22-bookworm

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential clang llvm pkg-config libelf-dev zlib1g-dev \
        python3 bpftool ca-certificates curl git \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup
ENV CARGO_HOME=/opt/cargo
ENV PATH=/opt/cargo/bin:${PATH}

RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none

COPY rust-toolchain.toml /toolchain/rust-toolchain.toml
RUN cd /toolchain && rustup show active-toolchain

WORKDIR /work
