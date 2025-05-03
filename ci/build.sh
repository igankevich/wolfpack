#!/bin/sh

. ./ci/apt.sh

toolchain=stable
target=x86_64-unknown-linux-musl
nix_version=2.28.3

musl_install() {
    apt_get update
    apt_get install -y musl-tools jq curl
    export CC=musl-gcc
}

rustup_install() {
    rustup update --no-self-update "$toolchain"
    rustup target add --toolchain "$toolchain" "$target"
    rustup default "$toolchain"
}

nix_install() {
    run_curl -o "$workdir"/nix-install https://releases.nixos.org/nix/nix-"$nix_version"/install
    chmod +x "$workdir"/nix-install
    "$workdir"/nix-install --no-daemon --no-channel-add --no-modify-profile --yes
    export PATH="$PATH":"$HOME"/.nix-profile/bin:"$HOME"/.local/state/nix/profile/bin
}

wolfpack_build() {
    cargo build --release --target "$target" --quiet
}

wolfpack_tar_gz() {
    mkdir -p "$workdir"/wolfpack/bin
    mv -v target/"$target"/release/wolfpack "$workdir"/wolfpack/bin/
    tar -C "$workdir"/wolfpack -czvf "$PWD"/wolfpack.tar.gz .
}

nix_hash() {
    file="$1"
    printf "%s " "$file"
    nix-hash --sri --type sha256 --flat "$file"
}

nix_generate_hashes() {
    rm -f "$workdir"/nix-hashes
    {
        printf "# Release %s\n" "$GITHUB_REF_NAME"
        printf "## Nix hashes\n"
        printf '```'"\n"
        {
            nix_hash wolfpack.tar.gz
        } | column -t
        printf '```'"\n"
    } >>"$workdir"/nix-hashes
    cat "$workdir"/nix-hashes
}

run_curl() {
    curl --fail-with-body --location "$@"
}

wolfpack_release() {
    run_curl \
        -X POST \
        -H "Accept: application/vnd.github+json" \
        -H "Authorization: Bearer $GITHUB_TOKEN" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        "$GITHUB_API_URL"/repos/"$GITHUB_REPOSITORY"/releases \
        -o /tmp/response \
        -d '{
    "tag_name":"'"$GITHUB_REF_NAME"'",
    "target_commitish":"'"$GITHUB_SHA"'",
    "name":"'"$GITHUB_REF_NAME"'",
    "body":'"$(jq -sR <"$workdir"/nix-hashes)"',
    "draft":true,
    "prerelease":false,
    "generate_release_notes":true
}'
    cat /tmp/response
    release_id="$(jq -r .id /tmp/response)"
    {
        printf "# Release %s\n" "$GITHUB_REF_NAME"
        printf "<details><summary>Nix hashes</summary>\n\n"
        printf '```'"\n"
    } >>"$workdir"/assets
    # shellcheck disable=SC2043
    for file in wolfpack.tar.gz; do
        name="$(basename "$file")"
        run_curl \
            -o "$workdir"/response \
            -X POST \
            -H "Accept: application/vnd.github+json" \
            -H "Authorization: Bearer $GITHUB_TOKEN" \
            -H "X-GitHub-Api-Version: 2022-11-28" \
            -H "Content-Type: application/octet-stream" \
            "https://uploads.github.com/repos/$GITHUB_REPOSITORY/releases/$release_id/assets?name=$name" \
            --data-binary "@$file"
        asset_url="$(jq -r .url "$workdir"/response)"
        {
            printf '%s ' "$asset_url"
            nix_hash "$file"
        } >>"$workdir"/assets
    done
    printf '```'"\n\n</details>" >>"$workdir"/assets
    cat "$workdir"/assets
    # Add asset URLs to the release.
    run_curl \
        -X PATCH \
        -H "Accept: application/vnd.github+json" \
        -H "Authorization: Bearer $GITHUB_TOKEN" \
        -H "X-GitHub-Api-Version: 2022-11-28" \
        "$GITHUB_API_URL"/repos/"$GITHUB_REPOSITORY"/releases/"$release_id" \
        -o /tmp/response \
        -d '{
    "tag_name":"'"$GITHUB_REF_NAME"'",
    "target_commitish":"'"$GITHUB_SHA"'",
    "name":"'"$GITHUB_REF_NAME"'",
    "body":'"$(jq -sR <"$workdir"/assets)"',
    "draft":true,
    "prerelease":false,
    "generate_release_notes":true
}'
    cat /tmp/response
}

main() {
    . ./ci/preamble.sh

    # TODO
    #if test "$GITHUB_ACTIONS" = "true" && test "$GITHUB_REF_TYPE" != "tag"; then
    #    return
    #fi

    musl_install
    rustup_install
    wolfpack_build

    wolfpack_tar_gz
    nix_install
    nix_generate_hashes
    wolfpack_release
}

main
