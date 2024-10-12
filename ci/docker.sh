#!/bin/sh
. ./ci/preamble.sh
for suffix in '' '-debian'; do
    image=ghcr.io/igankevich/wolfpack-ci"$suffix":latest
    docker build --tag "$image" - <ci/Dockerfile"$suffix"
done
#docker push $image
