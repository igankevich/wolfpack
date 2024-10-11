FROM debian:bookworm

RUN export DEBIAN_FRONTEND=noninteractive \
    && apt-get -qq update \
    && apt-get -qq install --no-install-recommends --no-install-suggests \
    pre-commit \
    python3-pip \
    libclang-dev \
    shellcheck \
    clang \
    curl \
    lcov \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_HOME=/usr/local

RUN curl --proto '=https' --tlsv1.2 -o /tmp/rustup.sh -sSf https://sh.rustup.rs \
    && chmod +x /tmp/rustup.sh \
    && /tmp/rustup.sh --profile minimal --quiet -y

RUN rustup toolchain add nightly \
    --target x86_64-unknown-linux-gnu \
    --component rustfmt miri llvm-tools-preview
RUN rustup toolchain add 1.81.0 \
    --target x86_64-unknown-linux-gnu \
    --component clippy rustfmt llvm-tools-preview
RUN rustup target add --toolchain 1.81.0 \
    x86_64-unknown-linux-musl
RUN rustup default 1.81.0-x86_64-unknown-linux-gnu
RUN cargo install --quiet --no-default-features --features native-certs cargo-deny

RUN curl --location --silent --fail --output /usr/local/bin/shfmt https://github.com/mvdan/sh/releases/download/v3.8.0/shfmt_v3.8.0_linux_amd64 && \
    chmod +x /usr/local/bin/shfmt

COPY --from=ghcr.io/staex-io/cijail:latest / /usr/local

LABEL org.opencontainers.image.source=https://github.com/igankevich/wolfpack-ci
LABEL org.opencontainers.image.description="CI image"

CMD ["/bin/bash"]
