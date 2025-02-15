#!/bin/sh

build_openwrt() {
    version=23.05.5
    url=https://downloads.openwrt.org/releases/$version/targets/x86/64/openwrt-$version-x86-64-rootfs.tar.gz
    image=ghcr.io/igankevich/wolfpack-ci-openwrt-2:latest
    rootfs="$workdir"/openwrt-rootfs
    mkdir -p "$rootfs"
    curl --silent --fail --location "$url" | tar -xzf- -C "$workdir"
    mkdir -p "$workdir"/var/lock
    cat >"$workdir"/Dockerfile <<'EOF'
FROM scratch
COPY . /
CMD ["/bin/sh"]
LABEL org.opencontainers.image.source=https://github.com/igankevich/wolfpack
LABEL org.opencontainers.image.description="CI image"
EOF
    docker build --tag "$image" "$workdir"
    docker push "$image"
}

build_with_suffix() {
    suffix="$1"
    image=ghcr.io/igankevich/wolfpack-ci"$suffix":latest
    docker build --tag "$image" - <ci/Dockerfile"$suffix"
    docker push "$image"
}

build_other() {
    if test "$1" = "all"; then
        for suffix in '-lib' '-debian' '-freebsd' '-darling' '-wine'; do
            build_with_suffix "$suffix"
        done
    else
        build_with_suffix "$1"
    fi
}

main() {
    . ./ci/preamble.sh
    suffix="$1"
    case "$suffix" in
    openwrt) build_openwrt ;;
    *) build_other "$suffix" ;;
    esac
}

main "$@"
