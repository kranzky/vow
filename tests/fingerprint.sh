#!/bin/sh
# The verifier version is part of every verification cache key, so a cached result is
# trusted only by the verifier that proved it. Whenever the SMT the verifier emits
# changes, the version must change too: the change might fix a soundness bug, and then
# results proved before it must not be reused.
#
# This checks that rule. tests/smt.fingerprint holds the verifier version, then for each
# test program and example a hash of its source and a hash of the SMT emitted for it.
# If a program's source is unchanged but its SMT is not, the verifier changed, and the
# version must have changed too. `make fingerprint` (this script with --update)
# records the current hashes: after bumping verifier_version() in compiler/hash.vow, or
# after adding or editing programs.
cd "$(dirname "$0")/.." || exit 1
VOWC=${VOWC:-build/vowc}
version=$("$VOWC" version | sed -n 's/^vowc [^ ]* (\(.*\))$/\1/p')
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
sha() { (sha256sum "$1" 2>/dev/null || shasum -a 256 "$1") | cut -d' ' -f1; }
echo "$version" > "$tmp/new"
for f in tests/run/*.vow examples/*.vow; do
  rm -f "$tmp/one.smt2"
  VOW_SMT_DUMP="$tmp/one.smt2" "$VOWC" check "$f" --no-cache >/dev/null 2>&1
  touch "$tmp/one.smt2"
  echo "$f $(sha "$f") $(sha "$tmp/one.smt2")" >> "$tmp/new"
done
if [ "$1" = "--update" ]; then
  cp "$tmp/new" tests/smt.fingerprint
  echo "recorded the SMT fingerprint for $version"
  exit 0
fi
old_version=$(head -1 tests/smt.fingerprint)
changed=""
while read -r f src smt; do
  [ -n "$smt" ] || continue
  old=$(grep "^$f " tests/smt.fingerprint)
  [ -n "$old" ] || continue
  set -- $old
  if [ "$2" = "$src" ] && [ "$3" != "$smt" ]; then
    changed="$changed $f"
  fi
done < "$tmp/new"
if [ -z "$changed" ]; then
  if [ "$version" != "$old_version" ]; then
    echo "FAIL the verifier version is now $version: run 'make fingerprint' to record its SMT"
    exit 1
  fi
  echo "SMT fingerprint unchanged ($version)"
  exit 0
fi
if [ "$version" = "$old_version" ]; then
  echo "FAIL the verifier emits different SMT for unchanged programs:$changed"
  echo "     bump verifier_version() in compiler/hash.vow, then run 'make fingerprint'"
else
  echo "FAIL the verifier version is now $version: run 'make fingerprint' to record its SMT"
fi
exit 1
