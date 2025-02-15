#!/bin/sh

apt_get() {
    sudo --non-interactive env DEBIAN_FRONTEND=noninteractive apt-get "$@"
}

install_dependencies() {
    if test "$GITHUB_ACTIONS" != "true"; then
        return
    fi
    apt_get update -qq
    apt_get install -y moreutils
}

silent() {
    if test "$GITHUB_ACTIONS" = "true"; then
        chronic "$@"
    else
        "$@"
    fi
}

test_integration() {
    cargo test-deb
}

cargo_test_lib() {
    silent cargo test \
        --workspace \
        --no-fail-fast \
        --lib \
        --config "target.'cfg(target_os = \"linux\")'.runner=\"./ci/runner.sh $DOCKER_IMAGE\"" \
        -- "$@"
}

cargo_test_all() {
    export ARBTEST_BUDGET_MS=2000
    unset RUST_TEST_THREADS
    DOCKER_IMAGE="ghcr.io/igankevich/wolfpack-ci-lib:latest" cargo_test_lib --nocapture
    export ARBTEST_BUDGET_MS=10000
    export RUST_TEST_THREADS=1
    DOCKER_IMAGE="ghcr.io/igankevich/wolfpack-ci-openwrt-2:latest" cargo_test_lib --nocapture --ignored opkg
    DOCKER_IMAGE="ghcr.io/igankevich/wolfpack-ci-debian:latest" cargo_test_lib --nocapture --ignored dpkg apt
    DOCKER_IMAGE="ghcr.io/igankevich/wolfpack-ci-freebsd:latest" cargo_test_lib --nocapture --ignored bsd_pkg
    DOCKER_IMAGE="docker.io/fedora:latest" cargo_test_lib --nocapture --ignored rpm_ dnf
    unset ARBTEST_BUDGET_MS
    unset RUST_TEST_THREADS
}

main() {
    . ./ci/preamble.sh
    install_dependencies
    cargo_test_all
    #test_integration
}

main
