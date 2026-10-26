#!/bin/sh
# Runs every tests/run/*.vow and compares stdout with the matching .out file, and
# checks that every tests/fail/*.vow is rejected with the message in its first line
# ("// error: <text>").
cd "$(dirname "$0")" || exit 1
VOWC=${VOWC:-"$(pwd)/../bootstrap/target/debug/vowc"}
pass=0
fail=0
# With VOW_FROZEN set (the Rust bootstrap compiler), skip programs that use language
# features added after v0.3; they say so in a "// language: ..." line.
skip() { [ -n "$VOW_FROZEN" ] && grep -q '^// language:' "$1"; }
for f in run/*.vow; do
  skip "$f" && continue
  exp="${f%.vow}.out"
  got=$("$VOWC" run "$f" $VOWFLAGS 2>&1)
  if [ "$got" = "$(cat "$exp")" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL $f"
    echo "$got" | diff "$exp" - | head -20
  fi
done
for f in fail/*.vow; do
  [ -e "$f" ] || continue
  skip "$f" && continue
  want=$(head -1 "$f" | sed 's|^// error: ||')
  got=$("$VOWC" check "$f" 2>&1)
  if [ $? -ne 0 ] && printf '%s' "$got" | grep -qF -- "$want"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL $f (wanted: $want)"
    echo "$got" | head -10
  fi
done
for f in ../examples/*.vow; do
  skip "$f" && continue
  got=$("$VOWC" check "$f" 2>&1)
  if [ $? -eq 0 ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL $f (examples must verify)"
    echo "$got" | head -10
  fi
done
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
