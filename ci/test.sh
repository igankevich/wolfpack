#!/bin/sh

test_integration() {
    cargo_test_docker ghcr.io/igankevich/wolfpack-ci-debian:latest --nocapture --ignored dpkg apt
}

cargo_test_docker() {
    target=x86_64-unknown-linux-gnu
    image="$1"
    shift
    cargo test --config target."$target".runner=\""$PWD/ci/runner.sh $image"\" -- "$@"
}

main() {
    . ./ci/preamble.sh
    test_integration
}

main
