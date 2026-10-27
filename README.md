# Vow

Vow is a programming language designed for a machine author. Its guiding rule: every
function should be checkable from its signature alone. Effects are declared, contracts
are proved by the Z3 SMT solver at compile time, and nothing depends on counting or on
context far away.

```
fn is_sorted(xs: List<Int>) -> Bool do
  return forall i in 0..len(xs): forall j in i..len(xs): xs[i] <= xs[j]
end fn is_sorted

fn search(xs: List<Int>, target: Int) -> Int
  requires is_sorted(xs)
  ensures (-1 <= result) and (result < len(xs))
  ensures (result >= 0) implies (xs[result] == target)
  ensures (result == -1) implies (forall i in 0..len(xs): xs[i] != target)
do
  var lo = 0
  var hi = len(xs)
  while lo < hi
    invariant (0 <= lo) and (lo <= hi) and (hi <= len(xs))
    invariant forall i in 0..lo: xs[i] < target
    invariant forall i in hi..len(xs): xs[i] > target
    decreases hi - lo
  do
    let mid = lo + ((hi - lo) / 2)
    let x = xs[mid]                    // proved in bounds: no runtime check
    if x == target then
      return mid
    elif x < target then
      set lo = mid + 1
    else
      set hi = mid
    end if
  end while
  return -1
end fn search
```

If this compiles, binary search is correct for every sorted input: a returned index
holds the target, `-1` means the target is absent, and the loop terminates.

## What makes it different

- **Contracts are proved, not tested.** Refinement types (`Int where it >= 0`),
  `requires`, `ensures`, loop invariants, quantified properties (`forall i in
  0..len(xs): ...`), termination measures, index bounds and division by zero are proof
  obligations. When a proof fails you get a counterexample:

  ```
  index.vow:3:10: error: cannot prove index in bounds: 0 <= 0 < len(xs)
      counterexample: len(xs) = 0
  ```

- **Effects are part of the signature.** A function is pure unless it says
  `uses console`, `uses fs` or `uses proc`, and callers must declare a superset.
- **No precedence, no counting.** `a + b * c` is an error; write `a + (b * c)`. Every
  block has a labelled end (`end fn search`), so a missing delimiter fails locally.
- **Named arguments, no shadowing, no null, no exceptions, no implicit conversions.**
- **Mutable value semantics.** Values are immutable; `var` and `inout` parameters give
  you mutation without aliasing.
- **Typed holes.** Write `?name` and the compiler reports the expected type, the
  variables in scope and every fact it knows at that point.
- **Content-addressed verification.** A function is re-verified only when its body, or
  the *contracts* of what it calls, change.

## Standard library

`import "std/text.vow"` gives verified text utilities (`find`, `split`, `lines`,
`trim`, `replace`, `starts_with`, ...), each with a contract the verifier can use.

## Self-hosting

The compiler is written in Vow (`compiler/`, about 6,000 lines) and verified by itself:
every index, slice and refinement in it is proved, with no runtime-checked escape
hatches. `make bootstrap` rebuilds it with itself, starting from the C seed, and checks
the fixpoint: the compiler built by itself emits byte-identical C.

It was bootstrapped with a Rust compiler (`bootstrap/`), which is now frozen at the
v0.3 language. The test suite still runs it on every program written in that
language, and both compilers' verifiers must emit identical SMT for each of them.

## Getting started

With [Homebrew](https://brew.sh) (macOS or Linux), which also installs Z3:

```
brew install kranzky/vow/vow
```

Or with the install script, which downloads a prebuilt `vowc` for macOS or Linux and
checks its checksum:

```
curl -fsSL https://vow.kranzky.com/install.sh | sh
```

`vowc` also needs a C compiler (`cc`) and [Z3](https://github.com/Z3Prover/z3)
(`brew install z3` or `apt install z3`). Then:

```
vowc run hello.vow        # verify, compile and run
vowc check hello.vow      # verify only
vowc build hello.vow -o hello
```

Commands: `check`, `build -o <exe>`, `run`, `emit-c`, `hash`, `version`. Options:
`--json` (machine-readable diagnostics), `--no-verify`, `--no-cache`.

To build from source instead, you need only a C compiler and Z3:

```
make                                  # builds build/vowc from seed/vowc.c
build/vowc run examples/bsearch.vow
make test                             # test suite (also needs Rust for the frozen bootstrap compiler)
```

## Documents

- [`docs/SPEC.md`](docs/SPEC.md): the language specification
- [`docs/VOW_FOR_LLMS.md`](docs/VOW_FOR_LLMS.md): for language models writing Vow
- [`docs/VOW_FOR_HUMANS.md`](docs/VOW_FOR_HUMANS.md): what people gain when their LLM colleagues write Vow
- [`docs/COMPARISON.md`](docs/COMPARISON.md): how Vow compares with other languages, and where its ideas come from
- [`ROADMAP.md`](ROADMAP.md): milestones and the V1 scope

## Status

The current release is **v0.4.0**, which adds generics, `Result` and `try`; the main
branch adds structural `==` and `old(x)` (see the [changelog](CHANGELOG.md)). Vow is an experiment, not yet a V1. Notable gaps:
no floats or modules; termination is proved only where measures are given; Unix only.
See the roadmap.

## License

Public domain ([Unlicense](UNLICENSE)).
