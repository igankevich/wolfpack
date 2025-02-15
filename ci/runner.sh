#!/bin/sh

cleanup() {
    rm -rf "$workdir"
}

set -e
trap cleanup EXIT
image="$1"
shift
executable="$1"
shift
filename="$(basename "$executable")"
workdir="$(mktemp -d /dev/shm/.wolfpack-runner-XXXXXXXX)"
cp -v "$executable" "$workdir"/"$filename"
ldd "$executable" |
    while read -r lib _arrow path _rest; do
        if test -e "$path"; then
            cp -v "$path" "$workdir"/
            # Fix permissions.
            name="$(basename "$path")"
            chmod 755 "$workdir"/"$name"
        fi
        if test -e "$lib"; then
            cp -v "$lib" "$workdir"/
            # Fix permissions.
            name="$(basename "$lib")"
            chmod 755 "$workdir"/"$name"
        fi
    done
patchelf \
    --set-interpreter /wolfpack/ld-linux-x86-64.so.2 \
    --set-rpath \$ORIGIN \
    --force-rpath \
    "$workdir"/"$filename"
docker run \
    --rm \
    --volume "$workdir":/wolfpack \
    --volume "$PWD":/src \
    --privileged \
    --env ARBTEST_BUDGET_MS \
    --env RUST_TEST_THREADS \
    "$image" \
    /wolfpack/"$filename" "$@"
