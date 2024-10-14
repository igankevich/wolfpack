#!/bin/sh

test_integration() {
    cargo test-deb
}

main() {
    . ./ci/preamble.sh
    test_integration
}

main
