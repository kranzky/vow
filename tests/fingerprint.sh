#!/bin/sh
# The verifier version is part of every verification cache key, so a cached result is
# trusted only by the verifier that proved it. Whenever the SMT the verifier emits
# changes, the version must change too: the change might fix a soundness bug, and then
# results proved before it must not be reused.
#
# This checks that rule. tests/smt.fingerprint holds the verifier version and a hash of
# the SMT emitted for every test program and example. If the SMT changed but the
# version did not, it fails. `make fingerprint` (this script with --update) records a
# new fingerprint after the version is bumped in compiler/hash.vow.
cd "$(dirname "$0")/.." || exit 1
VOWC=${VOWC:-build/vowc}
version=$("$VOWC" version | sed -n 's/^vowc [^ ]* (\(.*\))$/\1/p')
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for f in tests/run/*.vow examples/*.vow; do
  VOW_SMT_DUMP="$tmp/one.smt2" "$VOWC" check "$f" --no-cache >/dev/null 2>&1
  { echo "; $f"; cat "$tmp/one.smt2" 2>/dev/null; } >> "$tmp/all.smt2"
done
hash=$( (sha256sum "$tmp/all.smt2" 2>/dev/null || shasum -a 256 "$tmp/all.smt2") | cut -d' ' -f1)
if [ "$1" = "--update" ]; then
  echo "$version $hash" > tests/smt.fingerprint
  echo "recorded $version $hash"
  exit 0
fi
read -r old_version old_hash < tests/smt.fingerprint
if [ "$hash" = "$old_hash" ] && [ "$version" = "$old_version" ]; then
  echo "SMT fingerprint unchanged ($version)"
  exit 0
fi
if [ "$version" = "$old_version" ]; then
  echo "FAIL the SMT the verifier emits has changed, but the verifier version is still $version:"
  echo "     bump verifier_version() in compiler/hash.vow, then run 'make fingerprint'"
else
  echo "FAIL the verifier version is now $version: run 'make fingerprint' to record its SMT"
fi
exit 1
