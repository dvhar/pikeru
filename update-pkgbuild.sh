#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <version>"
  exit 1
fi

TAG="$1"

# Read pkgname from the PKGBUILD
PKGNAME=$(grep -oP '^pkgname=\K.*' PKGBUILD)

URL="https://github.com/dvhar/$PKGNAME/archive/refs/tags/$TAG.tar.gz"

# Download the tarball to a temp file
TMPFILE=$(mktemp)
trap 'rm -f "$TMPFILE"' EXIT

echo "Downloading $URL ..."
curl -sL -o "$TMPFILE" "$URL"

# Compute the new sha512
SHA512=$(sha512sum "$TMPFILE" | awk '{print $1}')

# Update PKGBUILD: pkgver and sha512sums
sed -i \
  -e "s/^pkgver=.*/pkgver=$TAG/" \
  -e "s/^sha512sums=.*/sha512sums=('$SHA512')/" \
  PKGBUILD

echo "PKGBUILD updated: pkgver=$TAG, sha512=$SHA512"
