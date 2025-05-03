#!/bin/sh

apt_get() {
    sudo --non-interactive env DEBIAN_FRONTEND=noninteractive apt-get -qq "$@"
}
