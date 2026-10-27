#!/bin/sh
# Installs the Z3 release Vow is tested with (tested_z3_version in compiler/verify.vow)
# into a directory (default build/z3) and checks its SHA-256. Proofs that succeed or fail
# by a margin do so with any recent Z3, but whether a hard query times out can differ
# between versions, so CI uses exactly this one.
#
#   tools/install-z3.sh [dir]     then: export PATH="$PWD/build/z3:$PATH" (or VOW_Z3)
set -e
VERSION=5.3.2
dest=${1:-build/z3}
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) asset=z3-$VERSION-x64-glibc-2.39
    sum=26d463cddad3901cbf86e3868c784914539f4e3f9b4de1a0b8dc2969806d60e2 ;;
  Linux-aarch64 | Linux-arm64) asset=z3-$VERSION-arm64-glibc-2.38
    sum=30b6a718a6cfe1b2db056038cc32eda5410f6d02e5937622b5238645347f59d5 ;;
  Darwin-arm64) asset=z3-$VERSION-arm64-osx-13.3
    sum=524290057c943a044ec3d1372db65a9c93f333d9817d58191154c4719ff7f8ad ;;
  Darwin-x86_64) asset=z3-$VERSION-x64-osx-13.3
    sum=b911dc193f7447a14fba001e8e8a0c2866a544274bb003e76a9e864f03b4d557 ;;
  *)
    echo "no prebuilt Z3 $VERSION for $(uname -s) $(uname -m); see https://github.com/Z3Prover/z3/releases" >&2
    exit 1 ;;
esac
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/z3.zip" "https://github.com/Z3Prover/z3/releases/download/z3-$VERSION/$asset.zip"
got=$( (sha256sum "$tmp/z3.zip" 2>/dev/null || shasum -a 256 "$tmp/z3.zip") | cut -d' ' -f1)
if [ "$got" != "$sum" ]; then
  echo "checksum mismatch for $asset.zip: got $got" >&2
  exit 1
fi
unzip -q "$tmp/z3.zip" -d "$tmp"
mkdir -p "$dest"
cp "$tmp/$asset/bin/z3" "$dest/z3"
chmod +x "$dest/z3"
"$dest/z3" --version
