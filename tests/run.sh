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
# Proofs are kept in a store shared by every program (see "Content addressing" in
# docs/SPEC.md). The tests use an empty one of their own, so that what they prove does
# not depend on what was proved before.
VOW_CACHE=$(mktemp -d)
export VOW_CACHE
trap 'rm -rf "$VOW_CACHE"' EXIT
# With VOW_FROZEN set (the Rust bootstrap compiler), skip programs that use language
# features added after v0.3; they say so in a "// language: ..." line.
skip() { [ -n "$VOW_FROZEN" ] && grep -q '^// language:' "$1"; }
for f in run/*.vow; do
  skip "$f" && continue
  exp="${f%.vow}.out"
  got=$("$VOWC" run "$f" $VOWFLAGS 2>&1 < /dev/null)
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
    # The usual stack limit, so that panic/stack_overflow.vow overflows soon wherever
    # it runs.
    (ulimit -s 8192 2>/dev/null; "$tmpd/prog" >/dev/null 2>"$tmpd/err")
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
  # A file fmt cannot read, or crashes on, is a failure too; except that the tests of
  # syntax errors (in a fail/ directory) need not parse.
  unformatted=$(find ../compiler ../std ../examples ../tests ../tools ../bench -name '*.vow' ! -path '*/fmt/messy.vow' |
    while read -r f; do
      out=$("$VOWC" fmt "$f" --check 2>&1)
      st=$?
      [ "$st" -eq 0 ] && continue
      case "$out" in (*"not formatted"*) echo "$f: not formatted"; continue ;; esac
      case "$f" in (*/fail/*) [ "$st" -lt 126 ] && continue ;; esac
      echo "$f: vowc fmt failed with status $st"
    done)
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
  want=$(printf 'fetched shapes C\ncreated vow.pkg (commit it; the fetched packages in .vow/ need not be)\npinned shapes in vow.pkg\nfetched units C\n12 300\n12 300')
  [ "$got" = "$want" ] || { pkg_ok=false; echo "packages: got"; echo "$got" | head -10; }
  printf 'import "@nope/x.vow" as x\nfn main() do\nend fn main\n' > "$pdir/proj/e.vow"
  (cd "$pdir/proj" && "$VOWC" check e.vow 2>&1 | grep -qF "the package 'nope' is not listed in vow.pkg") || pkg_ok=false
  rm -rf "$pdir/proj/.vow"
  (cd "$pdir/proj" && "$VOWC" check src/main.vow 2>&1 | grep -qF "the package 'shapes' has not been fetched") || pkg_ok=false
  (cd "$pdir/proj" && "$VOWC" fetch >/dev/null && "$VOWC" check src/main.vow >/dev/null) || pkg_ok=false
  # A short name is the branch or the tag of exactly that name: `main` is not
  # feature/main, an annotated tag gives its commit, and a name that is both a branch
  # and a tag is refused until it is written in full.
  mkdir -p "$pdir/refs" "$pdir/pin" && git init -q "$pdir/refs" && (
    cd "$pdir/refs" && g="git -c user.email=t@example.com -c user.name=t" &&
    git symbolic-ref HEAD refs/heads/main && cp ../units/units.vow . && git add -A &&
    $g commit -qm one && $g tag -a v1 -m v1 && git checkout -q -b feature/main &&
    echo "// two" >> units.vow && $g commit -qam two && git tag both && git branch both main
  ) || pkg_ok=false
  one=$(git -C "$pdir/refs" rev-parse main)
  two=$(git -C "$pdir/refs" rev-parse feature/main)
  for pin in "main $one" "v1 $one" "feature/main $two" "refs/tags/both $two" "refs/heads/both $one"; do
    (cd "$pdir/pin" && "$VOWC" add lib "$pdir/refs" "${pin%% *}" >/dev/null 2>&1 &&
      grep -q "^lib .* ${pin##* }\$" vow.pkg) || { pkg_ok=false; echo "packages: ${pin%% *} is not pinned to ${pin##* }"; }
  done
  (cd "$pdir/pin" && "$VOWC" add lib "$pdir/refs" both 2>&1 | grep -qF "'both' is both a branch and a tag") || pkg_ok=false
  (cd "$pdir/pin" && grep -q "^lib .* $one\$" vow.pkg) || pkg_ok=false
  # A pin that cannot be fetched is not written over one that can.
  cp "$pdir/pin/vow.pkg" "$pdir/pin.before"
  (cd "$pdir/pin" && "$VOWC" add lib "$pdir/refs" 0123456789012345678901234567890123456789 2>&1 |
    grep -qF "has no commit 0123456789012345678901234567890123456789") || pkg_ok=false
  cmp -s "$pdir/pin/vow.pkg" "$pdir/pin.before" || { pkg_ok=false; echo "packages: a pin that cannot be fetched was written"; }
  # git would read a URL that starts with '-' as an option (this one runs a command).
  (cd "$pdir/pin" && "$VOWC" add x "--upload-pack=touch $pdir/ran" 2>&1 | grep -qF "is not a git URL") || pkg_ok=false
  [ ! -e "$pdir/ran" ] || { pkg_ok=false; echo "packages: a URL was read as an option to git"; }
  printf 'x -uecho %s\n' "$one" > "$pdir/dash.pkg"
  (mkdir "$pdir/dash" && cd "$pdir/dash" && cp ../dash.pkg vow.pkg && "$VOWC" fetch 2>&1 | grep -qF "'-uecho' is not a git URL") || pkg_ok=false
  # A relative path in vow.pkg is relative to vow.pkg, wherever vowc runs; given to
  # `vowc add` it is relative to the working directory.
  mkdir -p "$pdir/rel/deep" && (cd "$pdir/rel" && "$VOWC" add units ../units >/dev/null 2>&1 &&
    grep -q '^units \.\./units ' vow.pkg && rm -rf .vow && cd deep && "$VOWC" fetch >/dev/null 2>&1 &&
    "$VOWC" add again ../../units >/dev/null 2>&1 && grep -q "^again $pdir/units " ../vow.pkg) ||
    { pkg_ok=false; echo "packages: relative paths"; }
  # ... but not in a fetched package's own vow.pkg, which is in the store.
  mkdir "$pdir/nest" && (cd "$pdir/nest" && cp ../rel/vow.pkg . && cp ../units/units.vow . && git init -q . && git add -A &&
    git -c user.email=t@example.com -c user.name=t commit -qm nest) || pkg_ok=false
  (mkdir "$pdir/outer" && cd "$pdir/outer" && "$VOWC" add nest "$pdir/nest" 2>&1 | grep -qF "must give a URL") || pkg_ok=false
  # Imports do not leave a package: neither "@name/../..." nor, from a file in a
  # package, a relative path that ends outside it. Inside it, "../" is fine.
  mkdir -p "$pdir/wild/in" && (cd "$pdir/wild" &&
    printf 'fn two() -> Int do\n  return 2\nend fn two\n' > b.vow &&
    printf 'import "../b.vow" as b\n\nfn four() -> Int do\n  return b.two() + b.two()\nend fn four\n' > in/a.vow &&
    printf 'import "../../../../secret.vow" as s\n\nfn leak() -> Int do\n  return s.secret()\nend fn leak\n' > esc.vow &&
    git init -q . && git add -A && git -c user.email=t@example.com -c user.name=t commit -qm wild) || pkg_ok=false
  mkdir "$pdir/app" && (cd "$pdir/app" && "$VOWC" add wild "$pdir/wild" >/dev/null 2>&1 &&
    printf 'fn secret() -> Int do\n  return 42\nend fn secret\n' > secret.vow &&
    printf 'import "@wild/in/a.vow" as a\n\nfn main() uses console do\n  print(int_to_str(a.four()) ++ "\\n")\nend fn main\n' > ok.vow &&
    printf 'import "@wild/../../../../secret.vow" as s\n\nfn main() uses console do\n  print(int_to_str(s.secret()) ++ "\\n")\nend fn main\n' > up.vow &&
    printf 'import "@wild/esc.vow" as e\n\nfn main() uses console do\n  print(int_to_str(e.leak()) ++ "\\n")\nend fn main\n' > out.vow &&
    [ "$("$VOWC" run ok.vow 2>&1)" = 4 ] &&
    "$VOWC" check up.vow 2>&1 | grep -qF "leaves the package" &&
    "$VOWC" check out.vow 2>&1 | grep -qF "is outside the package") || { pkg_ok=false; echo "packages: imports that leave a package"; }
  rm -rf "$pdir"
  if $pkg_ok; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL packages"
  fi
fi
# The proof store reuses a proof only while everything it relied on is
# unchanged: each tests/cache/<case>/before/ verifies (adding to the store), then the
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
# ... and it reuses every proof whose queries are unchanged: after an edit, only the
# functions whose proofs the edit reaches are sent to Z3 (VOW_SMT_DUMP holds what is
# sent). Each step edits reuse/main.vow and names the functions that must be proved.
# The store is shared: a second program with the same functions proves nothing.
if [ -z "$VOW_FROZEN" ]; then
  rdir=$(mktemp -d)
  cp reuse/main.vow "$rdir/"
  reuse_ok=true
  # proved <what was done> <the functions that must be sent to Z3> [file] [options]
  proved() {
    VOW_CACHE="$rdir/store" VOW_SMT_DUMP="$rdir/dump" "$VOWC" check "${3:-$rdir/main.vow}" $4 >/dev/null 2>&1 ||
      { reuse_ok=false; echo "reuse: $1: does not verify"; }
    got=$(sed -n 's/^(push 1) ; fn //p' "$rdir/dump" | tr '\n' ' ')
    [ "$got" = "$2" ] || { reuse_ok=false; echo "reuse: $1: proved [$got], expected [$2]"; }
  }
  all="clamp twice capped origin main "
  proved "--no-cache" "$all" "" --no-cache
  proved "the first check (--no-cache keeps nothing)" "$all"
  proved "no edit" ""
  mkdir "$rdir/elsewhere" && cp "$rdir/main.vow" "$rdir/elsewhere/other.vow"
  proved "the same functions in another program" "" "$rdir/elsewhere/other.vow"
  printf '\n// A comment.\nstruct Unused\n  a: Int,\nend struct Unused\n' >> "$rdir/main.vow"
  proved "an unused struct" ""
  sed -i.bak 's/return n + n/return n * 2/' "$rdir/main.vow"
  proved "a body, its contract unchanged" "twice "
  sed -i.bak 's/return 10/return 20/' "$rdir/main.vow"
  proved "the body of a function specifications inline" "capped "
  sed -i.bak 's/x: Int where it >= 0,/x: Int where it >= (0 - 5),/' "$rdir/main.vow"
  proved "a field refinement" "origin "
  sed -i.bak 's/ensures result >= 0/ensures (result >= 0) and (result >= n)/' "$rdir/main.vow"
  proved "a contract" "clamp origin main "
  # A proof that failed is not kept: the function is sent again.
  sed -i.bak 's/return n \* 2/return n - 1/' "$rdir/main.vow"
  for again in 1 2; do
    VOW_CACHE="$rdir/store" VOW_SMT_DUMP="$rdir/dump" "$VOWC" check "$rdir/main.vow" >/dev/null 2>&1 &&
      { reuse_ok=false; echo "reuse: a false postcondition verified"; }
    grep -q '^(push 1) ; fn twice$' "$rdir/dump" || { reuse_ok=false; echo "reuse: a failed proof was kept"; }
  done
  sed -i.bak 's/return n - 1/return n * 2/' "$rdir/main.vow"
  # Each function is proved by a solver of its own, under a limit on work and not on
  # time, so that an answer depends neither on what else was sent nor on the machine:
  # the script for two functions is the two scripts for one, one after the other.
  printf 'fn a(n: Int) -> Int\n  ensures result >= n\ndo\n  let m = n\n  return m\nend fn a\n\nfn b(n: Int where it >= 0) -> Int\n  ensures result >= 0\ndo\n  let m = n\n  return m\nend fn b\n' > "$rdir/a.vow"
  printf '\nfn main() do\n  let x = a(1)\nend fn main\n' >> "$rdir/a.vow"
  sed 's/fn a(/fn a0(/; s/end fn a$/end fn a0/; s/a(1)/b(1)/' "$rdir/a.vow" > "$rdir/b.vow"
  sed 's/fn b(/fn b0(/; s/end fn b$/end fn b0/' "$rdir/a.vow" > "$rdir/a1.vow"
  for one in a b a1; do
    VOW_SMT_DUMP="$rdir/$one.smt2" "$VOWC" check "$rdir/$one.vow" --no-cache >/dev/null 2>&1 || { reuse_ok=false; echo "reuse: $one.vow does not verify"; }
  done
  part() { awk -v want="$2" '/^\(reset\)$/ { on = 0; held = $0; next } /^\(push 1\) ; fn / { on = ($5 == want); if (on) print held } on' "$1"; }
  part "$rdir/a.smt2" a > "$rdir/in_both"
  part "$rdir/a1.smt2" a > "$rdir/alone"
  [ -s "$rdir/alone" ] && cmp -s "$rdir/in_both" "$rdir/alone" || { reuse_ok=false; echo "reuse: a function's queries depend on what else is sent"; }
  grep -q '^(set-option :rlimit [0-9]*)$' "$rdir/a.smt2" || { reuse_ok=false; echo "reuse: queries are not bounded by work"; }
  [ "$(grep -c '^(reset)$' "$rdir/a.smt2")" = 2 ] || { reuse_ok=false; echo "reuse: not one solver for each function"; }
  # A solver that stops answering is ended by its own limit, and vowc says so.
  printf '#!/bin/sh\ncat > /dev/null\necho sat\necho timeout\n' > "$rdir/stuck" && chmod +x "$rdir/stuck"
  VOW_Z3="$rdir/stuck" "$VOWC" check "$rdir/a.vow" --no-cache 2>&1 | grep -q 'Z3 answered 1 of 4 queries and then stopped answering' ||
    { reuse_ok=false; echo "reuse: a solver that stops is not reported"; }
  # With every proof in the store, Z3 is not needed at all.
  VOW_CACHE="$rdir/store" VOW_Z3=/nonexistent/z3 "$VOWC" check "$rdir/main.vow" >/dev/null 2>&1 ||
    { reuse_ok=false; echo "reuse: a check with every proof kept ran Z3"; }
  # A proof is one file in $VOW_CACHE/proofs/<verifier version>/<two characters>/.
  [ "$(find "$rdir/store/proofs" -type f | wc -l)" -gt 4 ] || { reuse_ok=false; echo "reuse: the store is not where it should be"; }
  [ -z "$(find "$rdir" -name '*.vowcache')" ] || { reuse_ok=false; echo "reuse: a .vowcache file was written"; }
  # Without a home there is no store, and checking works.
  env -u VOW_CACHE -u XDG_CACHE_HOME -u HOME "$VOWC" check "$rdir/main.vow" >/dev/null 2>&1 ||
    { reuse_ok=false; echo "reuse: a check without a home failed"; }
  rm -rf "$rdir"
  if $reuse_ok; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL the cache proved more or less than an edit reaches"
  fi
fi
# A proof made with VOW_SMT_V03 (the v0.3 encoding, for differential.sh) is not cached:
# that encoding accepts fail/unreal_struct.vow, and a normal run after it must not.
if [ -z "$VOW_FROZEN" ]; then
  cdir=$(mktemp -d)
  cp fail/unreal_struct.vow "$cdir/main.vow"
  VOW_SMT_V03=1 "$VOWC" check "$cdir/main.vow" >/dev/null 2>&1
  if "$VOWC" check "$cdir/main.vow" 2>&1 | grep -qF "cannot prove assertion: x > 0"; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL a proof made with VOW_SMT_V03 was reused from the cache"
  fi
  rm -rf "$cdir"
fi
# A program that cannot write its standard output panics instead of exiting 0.
if [ -z "$VOW_FROZEN" ]; then
  odir=$(mktemp -d)
  printf 'fn main() uses console do\n  print("lost\\n")\nend fn main\n' > "$odir/out.vow"
  "$VOWC" build "$odir/out.vow" -o "$odir/out" >/dev/null 2>&1
  got=$("$odir/out" 2>&1 >&-; echo "status $?")
  if [ "$got" = "$(printf 'vow: panic: cannot write to standard output\nstatus 101')" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL a closed standard output: $got"
  fi
  rm -rf "$odir"
fi
# build refuses to write the executable over its source: a source file with no
# extension is also the default name of its executable.
if [ -z "$VOW_FROZEN" ]; then
  odir=$(mktemp -d)
  printf 'fn main() uses console do\n  print("kept\\n")\nend fn main\n' > "$odir/prog"
  cp "$odir/prog" "$odir/before"
  got=$(cd "$odir" && "$VOWC" build prog 2>&1; echo "status $?"; "$VOWC" build ./prog -o prog 2>&1; echo "status $?")
  want="vowc: the executable would overwrite the source file 'prog': name it with -o
status 2
vowc: the executable would overwrite the source file './prog': name it with -o
status 2"
  if [ "$got" = "$want" ] && cmp -s "$odir/prog" "$odir/before" && (cd "$odir" && "$VOWC" build prog -o out >/dev/null 2>&1 && [ "$(./out)" = kept ]); then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL building a source file with no extension: $got"
  fi
  rm -rf "$odir"
fi
# vowc run gives the program vowc's own standard input and its exit status.
if [ -z "$VOW_FROZEN" ]; then
  odir=$(mktemp -d)
  cat > "$odir/in.vow" <<'VOW'
fn main() uses console, proc do
  match read_line()
  case Some(value: l) then
    print("got " ++ l ++ "\n")
    exit(7)
  case None then
    print("no input\n")
  end match
end fn main
VOW
  got=$(echo hello | "$VOWC" run "$odir/in.vow" 2>&1; echo "status $?"; "$VOWC" run "$odir/in.vow" 2>&1 < /dev/null; echo "status $?")
  want="got hello
status 7
no input
status 0"
  if [ "$got" = "$want" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL vowc run with piped input: $got"
  fi
  rm -rf "$odir"
fi
# Nothing can be more than 1000 levels deep (max_depth in compiler/parser.vow): the
# passes recurse over the tree, and deeper input used to overflow the stack. A program
# that is as deep as allowed in every way builds and runs; one level more, or 40,000,
# is an error; and the output of tools/embed.vow for a 30,000-line file is accepted.
if [ -z "$VOW_FROZEN" ]; then
  ddir=$(mktemp -d)
  # rep <n> <text>: the text n times (awk turns a \n in it into a newline).
  rep() { awk -v n="$1" -v s="$2" 'BEGIN { for (i = 0; i < n; i++) printf "%s", s }'; }
  # deep <n>: blocks nested n deep around a sum nested n deep and an 'and' chain of n terms.
  deep() {
    printf 'fn id(x: Int) -> Int do\n  return x\nend fn id\nfn main() uses console, proc do\n  let c = len(args()) >= 0\n'
    rep $(($1 - 2)) 'if c then\n'
    printf '  let x = '; rep $(($1 - 3)) '(1 + '; printf 'id(1)'; rep $(($1 - 3)) ')'
    printf '\n  let z = c'; rep $(($1 - 1)) ' and c'
    printf '\n  if z then\n    print(int_to_str(x))\n  end if\n'
    rep $(($1 - 2)) 'end if\n'
    printf 'end fn main\n'
  }
  deep 1000 > "$ddir/deep.vow"
  got=$("$VOWC" run "$ddir/deep.vow" $VOWFLAGS 2>&1)
  if [ "$got" = 998 ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL a program 1000 levels deep: $(printf '%s' "$got" | head -3 | cut -c1-200)"
  fi
  # too_deep <name> <message> <program text>
  too_deep() {
    printf '%s' "$3" > "$ddir/$1.vow"
    got=$("$VOWC" check "$ddir/$1.vow" 2>&1)
    if [ $? -eq 1 ] && printf '%s' "$got" | grep -qF -- "$2 more than 1000 levels deep"; then
      pass=$((pass + 1))
    else
      fail=$((fail + 1)); echo "FAIL $1 (wanted: $2 more than 1000 levels deep): $(printf '%s' "$got" | head -3 | cut -c1-200)"
    fi
  }
  head='fn main() uses console do
'
  tail='
end fn main
'
  too_deep chain 'this expression is' "$head  print(\"a\"$(rep 1000 ' ++ "a"'))$tail"
  too_deep chain_long 'this expression is' "$head  print(\"a\"$(rep 40000 ' ++ "a"'))$tail"
  too_deep sum 'this expression is' "$head  let x = 1$(rep 1000 ' + 1')$tail"
  too_deep fields 'this expression is' "$head  let x = s$(rep 1000 '.a')$tail"
  too_deep parens 'this expression is nested' "$head  let x = $(rep 1001 '(')1$(rep 1001 ')')$tail"
  too_deep parens_long 'this expression is nested' "$head  let x = $(rep 40000 '(')1$(rep 40000 ')')$tail"
  too_deep open_only 'this expression is nested' "$head  let x = $(rep 40000 '(')$tail"
  too_deep negations 'this expression is nested' "$head  let x = $(rep 40000 'not ')true$tail"
  too_deep lists 'this expression is nested' "$head  let x = $(rep 40000 '[')1$(rep 40000 ']')$tail"
  too_deep calls 'this expression is nested' "$head  let x = $(rep 40000 'abs(')1$(rep 40000 ')')$tail"
  too_deep ifs 'these statements are' "$head$(rep 1001 'if true then\n')$(rep 1001 'end if\n')$tail"
  too_deep ifs_long 'these statements are' "$head$(rep 40000 'if true then\n')$(rep 40000 'end if\n')$tail"
  too_deep tries 'these statements are' "fn f() -> Option<Int> do
$(rep 1001 '  let _ = try f()\n')
  return Option.None
end fn f
$head$tail"
  too_deep types 'this type is' "fn f(x: $(rep 40000 'List<')Int$(rep 40000 '>')) do
end fn f
$head$tail"
  rep 30000 'a line of "text"\n' > "$ddir/text.txt"
  "$VOWC" run ../tools/embed.vow -- "$ddir/text.txt" text > "$ddir/text.vow" 2>&1
  printf 'fn main() uses console do\n  print(int_to_str(len(text())))\nend fn main\n' >> "$ddir/text.vow"
  got=$("$VOWC" run "$ddir/text.vow" $VOWFLAGS 2>&1)
  if [ "$got" = 510000 ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL embedding a 30,000-line file: $(printf '%s' "$got" | head -3 | cut -c1-200)"
  fi
  rm -rf "$ddir"
fi
# A file is loaded once however it is reached: here by its path, through a linked
# directory, through a link to the file and by an absolute path (it was loaded once
# for each, and its functions were "already declared"). An import that names a
# directory says so, and one spelled in another case than the file is an error where
# file names ignore case too (elsewhere the file is simply not found).
if [ -z "$VOW_FROZEN" ]; then
  ldir=$(mktemp -d)
  mkdir -p "$ldir/real" "$ldir/sub"
  printf 'fn one() -> Int do\n  return 1\nend fn one\n' > "$ldir/real/lib.vow"
  ln -s real "$ldir/link"
  ln -s real/lib.vow "$ldir/also.vow"
  printf 'import "real/lib.vow"\nimport "link/lib.vow"\nimport "also.vow" as also\nimport "%s/real/lib.vow" as full\nfn main() uses console do\n  print(int_to_str(one() + also.one() + full.one()))\nend fn main\n' "$ldir" > "$ldir/main.vow"
  got=$(cd "$ldir" && "$VOWC" run main.vow $VOWFLAGS 2>&1)
  if [ "$got" = 3 ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL one file imported by four names: $got"
  fi
  printf 'import "sub"\nfn main() do\nend fn main\n' > "$ldir/dir.vow"
  got=$(cd "$ldir" && "$VOWC" check dir.vow 2>&1; "$VOWC" check sub 2>&1)
  want="dir.vow:1:1: error: cannot open 'sub': it is a directory
sub:0:0: error: cannot open 'sub': it is a directory"
  if [ "$got" = "$want" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL a directory in place of a source file: $got"
  fi
  printf 'import "real/LIB.vow"\nfn main() do\n  let x = one()\nend fn main\n' > "$ldir/case.vow"
  got=$(cd "$ldir" && "$VOWC" check case.vow 2>&1)
  want="case.vow:1:1: error: cannot open 'real/LIB.vow'"
  if [ -e "$ldir/real/LIB.vow" ]; then
    want="case.vow:1:1: error: cannot open 'real/LIB.vow': the name on disk is 'lib.vow'
    where file names are case-sensitive, this import finds nothing"
  fi
  if [ "$got" = "$want" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL an import in another case than the file: $got"
  fi
  rm -rf "$ldir"
fi
# --json output is JSON whatever bytes the source has: a byte that is not UTF-8 in a
# message becomes U+FFFD (it was copied, and a JSON parser rejected the line).
if [ -z "$VOW_FROZEN" ]; then
  jdir=$(mktemp -d)
  printf 'fn main() do\n  let s = "\\\351"\nend fn main\n' > "$jdir/bytes.vow"
  got=$(cd "$jdir" && "$VOWC" check bytes.vow --json 2>&1)
  want='{"kind":"error","file":"bytes.vow","line":2,"col":12,"message":"unknown escape '"'"'\\\ufffd'"'"'","notes":[]}'
  if [ "$got" = "$want" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL --json with a byte that is not UTF-8: $got"
  fi
  rm -rf "$jdir"
fi
# Large programs of ordinary shapes. The first is 3,000 type aliases, each defined by
# the next, 3,000 one-line functions, each calling the next (the last is not one that
# a specification could use, so none of them is), and a function of 20,000 locals:
# each of these took time that grew with the square or the cube of its size (the
# aliases 2 s for 2,000, the functions 10 s for 500, the locals 2.5 s for 40,000). The
# second is an if of 2,500 arms, a match of 5,000 cases and 600 'try' statements in one
# function, none of which a C compiler accepted: the arms and cases were nested one
# inside the other, and the tries made C whose size grew with the square of their
# number.
if [ -z "$VOW_FROZEN" ]; then
  sdir=$(mktemp -d)
  awk 'BEGIN {
    for (i = 0; i < 3000; i++) printf "type A%d = A%d\n", i, i + 1
    print "type A3000 = Int"
    for (i = 0; i < 2999; i++) printf "fn f%d(x: Int) -> Int do\n  return f%d(x) + 1\nend fn f%d\n", i, i + 1, i
    print "fn f2999(x: Int) -> Int do\n  return x + len(\"a\" ++ \"b\")\nend fn f2999"
    print "fn locals() -> Int do"
    for (i = 0; i < 20000; i++) printf "  let v%d = %d\n", i, i
    print "  return v0 + v19999\nend fn locals"
    print "fn main() uses console do\n  let a: A0 = 7\n  print(int_to_str(f0(a) + locals()))\nend fn main"
  }' > "$sdir/front.vow"
  # Checked, not run: a C compiler is slow on the chain of functions.
  got=$("$VOWC" check "$sdir/front.vow" 2>&1)
  if [ "$got" = ok ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL a program of many aliases, functions and locals: $(printf '%s' "$got" | head -3 | cut -c1-200)"
  fi
  awk 'BEGIN {
    print "fn one() -> Result<Int, String> do\n  return Result.Ok(value: 1)\nend fn one"
    print "fn tries() -> Result<Int, String> do"
    for (i = 0; i < 600; i++) printf "  let v%d = try one()\n", i
    print "  return Result.Ok(value: v0 + v599)\nend fn tries"
    print "fn arm(n: Int) -> Int do\n  if n == 0 then\n    return 0"
    for (i = 1; i < 2500; i++) printf "  elif n == %d then\n    return %d\n", i, i
    print "  else\n    return -1\n  end if\nend fn arm"
    print "fn pick(n: Int) -> Int do\n  match n"
    for (i = 0; i < 5000; i++) printf "  case %d then\n    return %d\n", i, i
    print "  else\n    return -1\n  end match\nend fn pick"
    print "fn main() uses console do\n  match tries()\n  case Ok(value: t) then\n    print(int_to_str(arm(2499) + pick(4999) + arm(2500) + pick(5000) + t))\n  case Err(error: e) then\n    print(e)\n  end match\nend fn main"
  }' > "$sdir/back.vow"
  got=$("$VOWC" run "$sdir/back.vow" $VOWFLAGS 2>&1)
  if [ "$got" = 7498 ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1)); echo "FAIL a program with a long if, a long match and many tries: $(printf '%s' "$got" | head -3 | cut -c1-200)"
  fi
  rm -rf "$sdir"
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
