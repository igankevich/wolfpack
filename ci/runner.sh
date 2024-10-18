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
    while read -r _lib _arrow path _rest; do
        if test -z "$path"; then
            continue
        fi
        cp -v "$path" "$workdir"/
    done
patchelf \
    --set-interpreter /wolfpack/ld-linux-x86-64.so.2 \
    --set-rpath '$ORIGIN' \
    --force-rpath \
    "$workdir"/"$filename"
docker run --rm --volume "$workdir":/wolfpack --volume "$PWD":/src \
    "$image" \
    /wolfpack/"$filename" "$@"
