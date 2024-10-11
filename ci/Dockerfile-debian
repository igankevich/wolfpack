FROM debian:bookworm

RUN export DEBIAN_FRONTEND=noninteractive \
    && apt-get -qq update \
    && apt-get -qq install -y dpkg-dev debsig-verify lintian

ENV CARGO_HOME=/usr/local

RUN curl --proto '=https' --tlsv1.2 -o /tmp/rustup.sh -sSf https://sh.rustup.rs \
    && chmod +x /tmp/rustup.sh \
    && /tmp/rustup.sh --profile minimal --quiet -y
