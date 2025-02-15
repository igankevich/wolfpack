#!/bin/sh

build_openwrt() {
    version=23.05.5
    url=https://downloads.openwrt.org/releases/$version/targets/x86/64/openwrt-$version-x86-64-rootfs.tar.gz
    image=ghcr.io/igankevich/wolfpack-ci-openwrt:"$version"
    rootfs="$workdir"/openwrt-rootfs
    mkdir -p "$rootfs"
    curl --silent --fail --location "$url" | tar -xzf- -C "$workdir"
    mkdir -p "$workdir"/var/lock
    cat >"$workdir"/Dockerfile <<'EOF'
FROM scratch
COPY . /
CMD ["/bin/sh"]
EOF
    docker build --tag "$image" "$workdir"
}

build_other() {
    for suffix in '-lib' '-debian' '-freebsd' '-darling' '-wine'; do
        image=ghcr.io/igankevich/wolfpack-ci"$suffix":latest
        docker build --tag "$image" - <ci/Dockerfile"$suffix"
        docker push $image
    done
}

main() {
    . ./ci/preamble.sh
    build_openwrt
    build_other
}

main
