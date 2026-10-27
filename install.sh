#!/bin/sh
# Installs vowc, the Vow compiler, from the latest GitHub release.
#
#   curl -fsSL https://vow.kranzky.com/install.sh | sh
#
# VOW_INSTALL_DIR chooses the directory (default ~/.local/bin); VOW_VERSION picks a
# release tag (default: the latest). vowc also needs a C compiler and Z3.
set -eu

repo=kranzky/vow
dir=${VOW_INSTALL_DIR:-$HOME/.local/bin}

case "$(uname -s)" in
  Darwin) target=macos-universal ;;
  Linux)
    case "$(uname -m)" in
      x86_64 | amd64) target=linux-x86_64 ;;
      aarch64 | arm64) target=linux-arm64 ;;
      *) echo "vow: unsupported architecture $(uname -m)" >&2; exit 1 ;;
    esac ;;
  *) echo "vow: unsupported system $(uname -s); Vow supports macOS and Linux" >&2; exit 1 ;;
esac

tag=${VOW_VERSION:-$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" |
  sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)}
if [ -z "$tag" ]; then
  echo "vow: could not find the latest release" >&2
  exit 1
fi

name="vowc-${tag#v}-$target"
url="https://github.com/$repo/releases/download/$tag/$name.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "vow: downloading $name"
curl -fsSL "$url" -o "$tmp/$name.tar.gz"
curl -fsSL "$url.sha256" -o "$tmp/$name.tar.gz.sha256"
if command -v shasum >/dev/null 2>&1; then
  (cd "$tmp" && shasum -a 256 -c "$name.tar.gz.sha256" >/dev/null)
else
  (cd "$tmp" && sha256sum -c "$name.tar.gz.sha256" >/dev/null)
fi || { echo "vow: checksum mismatch; not installing" >&2; exit 1; }

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$dir"
cp "$tmp/$name/vowc" "$dir/vowc"
chmod +x "$dir/vowc"
echo "vow: installed $("$dir/vowc" version) to $dir/vowc"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "vow: add $dir to your PATH" ;;
esac
if ! command -v cc >/dev/null 2>&1; then
  echo "vow: vowc needs a C compiler: on macOS run 'xcode-select --install', on Linux install gcc"
fi
if ! command -v z3 >/dev/null 2>&1; then
  echo "vow: vowc needs Z3 to verify programs: 'brew install z3', or the tested release from"
  echo "     https://github.com/Z3Prover/z3/releases/tag/z3-5.3.2 (distribution packages are often old)"
fi
