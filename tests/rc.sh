#!/bin/sh
# Reference-counting safety: builds every test program, example and benchmark with the
# debug runtime (freed memory is poisoned and never reused; any use after free panics)
# and requires the expected output and zero objects live at exit. Finally the compiler
# compiles itself in the same mode.
cd "$(dirname "$0")/.." || exit 1
VOWC=${VOWC:-"$(pwd)/build/vowc"}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
pass=0
fail=0
check() { # name, binary, expected-output-file-or-empty, args...
  name=$1; bin=$2; expected=$3; shift 3
  out=$(VOW_LEAKS=1 "$bin" "$@" 2>"$tmp/err")
  if ! grep -q "^vow: 0 objects live at exit" "$tmp/err"; then
    fail=$((fail + 1)); echo "FAIL $name: $(tail -1 "$tmp/err")"; return
  fi
  if [ -n "$expected" ] && [ "$out" != "$(cat "$expected")" ]; then
    fail=$((fail + 1)); echo "FAIL $name: output differs"; return
  fi
  pass=$((pass + 1))
}
build() { VOW_CFLAGS=-DVOW_RC_DEBUG "$VOWC" build "$1" -o "$2" >/dev/null 2>"$tmp/build.err" || { echo "FAIL build $1"; cat "$tmp/build.err"; return 1; }; }
for f in tests/run/*.vow; do
  build "$f" "$tmp/prog" && check "$f" "$tmp/prog" "${f%.vow}.out"
done
for f in examples/bsearch.vow examples/isort.vow examples/escape.vow examples/calc.vow examples/config.vow examples/cursor.vow examples/wordfreq.vow examples/shapes.vow examples/results.vow; do
  build "$f" "$tmp/prog" && check "$f" "$tmp/prog" ""
done
cc -O2 -o "$tmp/fasta" bench/fasta.c && "$tmp/fasta" 2000 > "$tmp/input.fa"
build bench/binarytrees/binarytrees.vow "$tmp/bt" && check binarytrees "$tmp/bt" "" 10
build bench/fannkuch/fannkuch.vow "$tmp/fk" && check fannkuch "$tmp/fk" "" 8
build bench/knucleotide/knucleotide.vow "$tmp/kn" && check knucleotide "$tmp/kn" "" "$tmp/input.fa"
build tools/vowsite/vowsite.vow "$tmp/vowsite" && pass=$((pass + 1))
build compiler/vowc.vow "$tmp/vowc" && check "compiler compiling itself" "$tmp/vowc" "" emit-c compiler/vowc.vow --no-verify
echo "$pass passed, $fail failed (reference counting, debug runtime)"
[ "$fail" -eq 0 ]
