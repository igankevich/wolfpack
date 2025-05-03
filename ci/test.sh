#!/bin/sh

. ./ci/apt.sh

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

cargo_lints() {
    cargo fmt --all --check
    cargo clippy --quiet --all-targets --all-features --workspace -- -D warnings
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
    if test -n "$1"; then
        export ARBTEST_BUDGET_MS=10000
        export RUST_TEST_THREADS=1
        name="$1"
        shift
        cargo_test_lib "$name" --nocapture --ignored "$@"
        return
    fi
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

wolfpack() {
    ./target/debug/wolfpack "$@"
}

run_in() {
    root="$1"
    shift
    uid="$(id -u)"
    if test "$uid" = "0" || test -e /.dockerenv; then
        chroot "$root" "$@"
    elif test "$GITHUB_ACTIONS" = "true"; then
        sudo --non-interactive chroot "$root" "$@"
    else
        unshare -r /bin/sh -c "chroot $root $*"
    fi
}

wolfpack_build_project_test() {
    cargo build --bin wolfpack
    rm -rf "$workdir"/dummy
    rm -rf "$workdir"/out
    cargo new "$workdir"/dummy
    wolfpack build-project "$workdir"/dummy "$workdir"/out
    root="$workdir"/out/dummy/default/rootfs
    run_in "$root" /opt/dummy/bin/dummy
}

main() {
    . ./ci/preamble.sh
    install_dependencies
    cargo_lints
    cargo_test_all "$@"
    wolfpack_build_project_test
}

main "$@"
