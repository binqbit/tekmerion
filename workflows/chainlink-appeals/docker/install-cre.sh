#!/bin/sh
set -eu

# The glibc 2.35 release runs on the image's Debian Bookworm runtime.
version=1.39.0
case "$(dpkg --print-architecture)" in
  amd64) arch=amd64; sha=398ca2c354d1814767583753dab226e08370d5ecc11589f50f8833fbf1607ed4 ;;
  arm64) arch=arm64; sha=c9451ba42031aee74085e70db15c40b815bc691c24a34c5a383875066c5a9a27 ;;
  *) echo 'CRE image supports linux/amd64 and linux/arm64' >&2; exit 1 ;;
esac
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
archive="cre_linux_${arch}_ldd2-35.tar.gz"
curl --fail --location --silent --show-error \
  "https://github.com/smartcontractkit/cre-cli/releases/download/v${version}/${archive}" \
  --output "$temporary/$archive"
printf '%s  %s\n' "$sha" "$temporary/$archive" | sha256sum --check --status
tar -xzf "$temporary/$archive" -C "$temporary"
install -m 0755 "$temporary/cre_v${version}_linux_${arch}" /usr/local/bin/cre
cre version
