#!/bin/sh
# Runs every tests/run/*.vow and compares stdout with the matching .out file; checks
# that every tests/fail/*.vow is rejected with the message in its first line
# ("// error: <text>"); compares `check --totality` on tests/total/*.vow with the
# matching .out file; and checks that each tests/panic/*.vow program, built (with the
# flags on its "// vowflags:" line) and run, panics with the message in its .err file.
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
for f in total/*.vow; do
  skip "$f" && continue
  exp="${f%.vow}.out"
  got=$("$VOWC" check "$f" --totality 2>&1)
  if [ "$got" = "$(cat "$exp")" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL $f"
    echo "$got" | diff "$exp" - | head -20
  fi
done
tmpd=$(mktemp -d)
for f in panic/*.vow; do
  skip "$f" && continue
  flags=$(sed -n 's|^// vowflags: ||p' "$f")
  exp="${f%.vow}.err"
  if "$VOWC" build "$f" -o "$tmpd/prog" $flags >/dev/null 2>&1; then
    "$tmpd/prog" >/dev/null 2>"$tmpd/err"
    got=$(cat "$tmpd/err")
  else
    got="(did not build)"
  fi
  if [ "$got" = "$(cat "$exp")" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL $f"
    echo "$got" | diff "$exp" - | head -10
  fi
done
rm -rf "$tmpd"
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
# The pinned Z3 version must be the same in the compiler and the installer.
pinned=$(sed -n 's/^VERSION=//p' ../tools/install-z3.sh)
if grep -A1 'fn tested_z3_version' ../compiler/verify.vow | grep -q "\"$pinned\""; then
  pass=$((pass + 1))
else
  fail=$((fail + 1)); echo "FAIL tools/install-z3.sh pins Z3 $pinned, but tested_z3_version() differs"
fi
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
