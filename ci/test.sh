#!/bin/sh

test_integration() {
    cargo test-deb
}

cargo_test_lib() {
    cargo test \
        --workspace \
        --no-fail-fast \
        --lib \
        --config "target.'cfg(target_os = \"linux\")'.runner=\"./ci/runner.sh $DOCKER_IMAGE\"" \
        -- "$@"
}

main() {
    . ./ci/preamble.sh
    DOCKER_IMAGE="ghcr.io/igankevich/wolfpack-ci-lib:latest" cargo_test_lib --nocapture
    DOCKER_IMAGE="ghcr.io/igankevich/wolfpack-ci-openwrt:latest" cargo_test_lib --nocapture --ignored opkg
    #test_integration
}

main
