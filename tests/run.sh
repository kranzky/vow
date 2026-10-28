#!/bin/sh
# Runs every tests/run/*.vow and compares stdout with the matching .out file; checks
# that every tests/fail/*.vow is rejected with the message in its first line
# ("// error: <text>"); compares `check --totality` on tests/total/*.vow with the
# matching .out file; and checks that each tests/panic/*.vow program, built (with the
# flags on its "// vowflags:" line) and run, panics with the message in its .err file;
# then vowc test, vowc fmt, and the pinned Z3 version.
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
# vowc test (not in the frozen bootstrap compiler): its report on each tests/vtest/*_test.vow.
if [ -z "$VOW_FROZEN" ]; then
  for f in vtest/*_test.vow; do
    got=$("$VOWC" test "$f" 2>&1)
    if [ "$got" = "$(cat "${f%.vow}.out")" ]; then
      pass=$((pass + 1))
    else
      fail=$((fail + 1)); echo "FAIL $f"; echo "$got" | diff "${f%.vow}.out" - | head -10
    fi
  done
fi
# vowc fmt (not in the frozen bootstrap compiler): the messy input formats to the
# expected layout, which is already formatted, and every Vow file in the repository is.
if [ -z "$VOW_FROZEN" ]; then
  fdir=$(mktemp -d)
  cp fmt/messy.vow "$fdir/messy.vow"
  "$VOWC" fmt "$fdir/messy.vow" >/dev/null 2>&1
  if cmp -s "$fdir/messy.vow" fmt/messy.expected && "$VOWC" fmt "$fdir/messy.vow" --check >/dev/null 2>&1; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL fmt/messy.vow"; diff fmt/messy.expected "$fdir/messy.vow" | head -10
  fi
  rm -rf "$fdir"
  unformatted=$(find ../compiler ../std ../examples ../tests ../tools ../bench -name '*.vow' ! -path '*/fmt/messy.vow' |
    while read -r f; do "$VOWC" fmt "$f" --check 2>/dev/null | grep 'not formatted'; done)
  if [ -z "$unformatted" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL some files are not formatted:"; echo "$unformatted"
  fi
fi
# Packages (not in the frozen bootstrap compiler; needs git): two local repositories,
# shapes depending on units, a project that adds shapes (and so fetches units too),
# and the errors for a package that is not listed or not fetched.
if [ -z "$VOW_FROZEN" ] && command -v git >/dev/null 2>&1; then
  pdir=$(mktemp -d)
  pkg_ok=true
  mkdir -p "$pdir/units" "$pdir/shapes" "$pdir/proj/src"
  cp packages/units.vow "$pdir/units/" && git init -q "$pdir/units" && git -C "$pdir/units" add -A &&
    git -C "$pdir/units" -c user.email=t@example.com -c user.name=t commit -qm units || pkg_ok=false
  cp packages/shapes.vow "$pdir/shapes/"
  (cd "$pdir/shapes" && "$VOWC" add units "$pdir/units" >/dev/null && rm -rf .vow) || pkg_ok=false
  git init -q "$pdir/shapes" && git -C "$pdir/shapes" add -A &&
    git -C "$pdir/shapes" -c user.email=t@example.com -c user.name=t commit -qm shapes || pkg_ok=false
  cp packages/main.vow "$pdir/proj/src/"
  got=$(cd "$pdir/proj" && "$VOWC" add shapes "$pdir/shapes" | sed 's/ [0-9a-f]\{7\} / C /; s| from .*||; s| to .* in | in |'
    "$VOWC" run src/main.vow --no-cache 2>&1 && cd src && "$VOWC" run main.vow 2>&1)
  want=$(printf 'created vow.pkg (commit it; the fetched packages in .vow/ need not be)\npinned shapes in vow.pkg\nfetched shapes C\nfetched units C\n12 300\n12 300')
  [ "$got" = "$want" ] || { pkg_ok=false; echo "packages: got"; echo "$got" | head -10; }
  printf 'import "@nope/x.vow" as x\nfn main() do\nend fn main\n' > "$pdir/proj/e.vow"
  (cd "$pdir/proj" && "$VOWC" check e.vow 2>&1 | grep -qF "the package 'nope' is not listed in vow.pkg") || pkg_ok=false
  rm -rf "$pdir/proj/.vow"
  (cd "$pdir/proj" && "$VOWC" check src/main.vow 2>&1 | grep -qF "the package 'shapes' has not been fetched") || pkg_ok=false
  (cd "$pdir/proj" && "$VOWC" fetch >/dev/null && "$VOWC" check src/main.vow >/dev/null) || pkg_ok=false
  rm -rf "$pdir"
  if $pkg_ok; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL packages"
  fi
fi
# The verification cache reuses a proof only while everything it relied on is
# unchanged: each tests/cache/<case>/before/ verifies (filling the cache), then the
# files in after/ replace them, and main.vow must be rejected with the message on the
# first line of whichever main.vow is in use.
if [ -z "$VOW_FROZEN" ]; then
  for case in cache/*/; do
    cdir=$(mktemp -d)
    cp "$case"before/*.vow "$cdir/"
    if ! "$VOWC" check "$cdir/main.vow" >/dev/null 2>&1; then
      fail=$((fail + 1)); echo "FAIL $case: before/ does not verify"
    else
      cp "$case"after/*.vow "$cdir/"
      want=$(sed -n '1s|^// error: ||p' "$cdir/main.vow")
      if "$VOWC" check "$cdir/main.vow" 2>&1 | grep -qF "$want"; then
        pass=$((pass + 1))
      else
        fail=$((fail + 1)); echo "FAIL $case: after/ was not rejected with: $want"
      fi
    fi
    rm -rf "$cdir"
  done
fi
# The pinned Z3 version must be the same in the compiler and the installer.
pinned=$(sed -n 's/^VERSION=//p' ../tools/install-z3.sh)
if grep -A1 'fn tested_z3_version' ../compiler/verify.vow | grep -q "\"$pinned\""; then
  pass=$((pass + 1))
else
  fail=$((fail + 1)); echo "FAIL tools/install-z3.sh pins Z3 $pinned, but tested_z3_version() differs"
fi
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
