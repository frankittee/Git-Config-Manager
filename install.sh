#!/bin/sh

set -eu

repository="frankittee/Git-Config-Manager"
install_dir="${G_INSTALL_DIR:-${HOME}/.local/bin}"
version="${G_VERSION:-latest}"

fail() {
    echo "g installer: $*" >&2
    exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"

case "$(uname -s)" in
    Linux)
        os="unknown-linux-musl"
        ;;
    Darwin)
        os="apple-darwin"
        ;;
    *)
        fail "unsupported operating system: $(uname -s)"
        ;;
esac

case "$(uname -m)" in
    x86_64 | amd64)
        arch="x86_64"
        ;;
    arm64 | aarch64)
        arch="aarch64"
        ;;
    *)
        fail "unsupported architecture: $(uname -m)"
        ;;
esac

if [ "$version" = "latest" ]; then
    release_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' \
        "https://github.com/${repository}/releases/latest")"
    version="${release_url##*/}"
else
    case "$version" in
        v*) ;;
        *) version="v${version}" ;;
    esac
fi

target="${arch}-${os}"
archive="g-${version}-${target}.tar.gz"
download_url="https://github.com/${repository}/releases/download/${version}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

echo "Downloading g ${version} for ${target}..."
curl -fsSL "${download_url}/${archive}" -o "${tmp_dir}/${archive}"
curl -fsSL "${download_url}/SHA256SUMS" -o "${tmp_dir}/SHA256SUMS"

expected_checksum="$(
    awk -v archive="$archive" '$2 == archive || $2 == "*" archive { print $1; exit }' \
        "${tmp_dir}/SHA256SUMS"
)"
[ -n "$expected_checksum" ] || fail "checksum not found for ${archive}"

if command -v sha256sum >/dev/null 2>&1; then
    actual_checksum="$(sha256sum "${tmp_dir}/${archive}" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
    actual_checksum="$(shasum -a 256 "${tmp_dir}/${archive}" | awk '{ print $1 }')"
else
    fail "sha256sum or shasum is required"
fi

[ "$actual_checksum" = "$expected_checksum" ] || fail "checksum verification failed"

tar -xzf "${tmp_dir}/${archive}" -C "$tmp_dir"
mkdir -p "$install_dir"
install -m 755 "${tmp_dir}/g" "${install_dir}/g"

echo "Installed g to ${install_dir}/g"
case ":${PATH}:" in
    *":${install_dir}:"*) ;;
    *)
        echo "Add ${install_dir} to PATH to run g."
        ;;
esac
