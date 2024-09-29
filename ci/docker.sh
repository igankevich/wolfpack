#!/bin/sh
. ./ci/preamble.sh
image=ghcr.io/igankevich/woflpack-ci-debian:latest
docker build --tag $image - <ci/Dockerfile
#docker push $image
