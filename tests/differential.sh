#!/bin/sh
# The bootstrap (Rust) and self-hosted verifiers must emit byte-identical SMT for every
# program in the v0.3 language. (Their C differs: only the self-hosted compiler emits reference counting.)
cd "$(dirname "$0")/.." || exit 1
RUST=${RUST_VOWC:-bootstrap/target/release/vowc}
VOWC=${VOWC:-build/vowc}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
pass=0
fail=0
for f in tests/run/*.vow examples/*.vow bench/*/*.vow tools/vowsite/vowsite.vow compiler/vowc.vow; do
  # The Rust bootstrap is frozen at the v0.3 language.
  grep -q '^// language:' "$f" && continue
  VOW_SMT_DUMP="$tmp/a.smt2" "$RUST" check "$f" --no-cache >/dev/null 2>&1
  VOW_SMT_DUMP="$tmp/b.smt2" "$VOWC" check "$f" --no-cache >/dev/null 2>&1
  if cmp -s "$tmp/a.smt2" "$tmp/b.smt2"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "DIFF $f"
  fi
done
echo "$pass identical, $fail different (SMT from both verifiers)"
[ "$fail" -eq 0 ]
