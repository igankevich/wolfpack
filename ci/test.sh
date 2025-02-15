#!/bin/sh

test_integration() {
    cargo test-deb
}

test_unit() {
    cargo test --workpace --quiet --no-run
    cargo test --workpace --no-fail-fast -- --nocapture
}

main() {
    . ./ci/preamble.sh
    test_unit
    #test_integration
}

main
