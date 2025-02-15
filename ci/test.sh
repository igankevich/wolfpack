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
    docker_image="$1"
    shift
    silent cargo test \
        --workspace \
        --lib \
        --config "target.'cfg(target_os = \"linux\")'.runner=\"./ci/runner.sh $docker_image\"" \
        -- "$@"
}

cargo_test_all() {
    export ARBTEST_BUDGET_MS=2000
    unset RUST_TEST_THREADS
    cargo_test_lib ghcr.io/igankevich/wolfpack-ci-lib:latest --nocapture
    export ARBTEST_BUDGET_MS=10000
    export RUST_TEST_THREADS=1
    cargo_test_lib ghcr.io/igankevich/wolfpack-ci-openwrt-2:latest --nocapture --ignored opkg
    cargo_test_lib ghcr.io/igankevich/wolfpack-ci-debian:latest --nocapture --ignored dpkg apt
    cargo_test_lib ghcr.io/igankevich/wolfpack-ci-freebsd:latest --nocapture --ignored bsd_pkg
    cargo_test_lib docker.io/fedora:latest --nocapture --ignored rpm_ dnf
    cargo_test_lib ghcr.io/igankevich/wolfpack-ci-darling:latest --nocapture --ignored darling_
    cargo_test_lib ghcr.io/igankevich/wolfpack-ci-wine:latest --nocapture --ignored msixmgr_
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
