# Vow

Vow is a programming language designed for a machine author. Its guiding rule: every
function should be checkable from its signature alone. Effects are declared, contracts
are proved by the Z3 SMT solver at compile time, and nothing depends on counting or on
context far away.

```
fn search(xs: List<Int>, target: Int) -> Int
  ensures result >= -1
  ensures result < len(xs)
do
  var lo = 0
  var hi = len(xs)
  while lo < hi
    invariant 0 <= lo
    invariant lo <= hi
    invariant hi <= len(xs)
  do
    let mid = lo + ((hi - lo) / 2)
    let x = xs[mid]                 // proved in bounds: no runtime check is emitted
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

## What makes it different

- **Contracts are proved, not tested.** Refinement types (`Int where it >= 0`),
  `requires`, `ensures`, loop invariants, index bounds and division by zero are proof
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

## Self-hosting

The compiler is written in Vow (`compiler/`, about 6,000 lines) and verified by itself:
every index, slice and refinement in it is proved, with no runtime-checked escape
hatches. It was bootstrapped with a Rust compiler (`bootstrap/`), and `make bootstrap`
checks the fixpoint: the Rust compiler, the self-hosted compiler and the self-hosted
compiler built by itself all emit byte-identical C.

## Getting started

You need a C compiler and [Z3](https://github.com/Z3Prover/z3) (`brew install z3`,
`apt install z3`).

```
make                                  # builds build/vowc from seed/vowc.c
build/vowc run examples/bsearch.vow   # verify, compile and run
build/vowc check examples/shapes.vow  # verify only
make test                             # test suite, with both compilers
```

Commands: `check`, `build -o <exe>`, `run`, `emit-c`, `hash`. Options: `--json`
(machine-readable diagnostics), `--no-verify`, `--no-cache`.

## Documents

- [`docs/SPEC.md`](docs/SPEC.md): the language specification
- [`docs/VOW_FOR_LLMS.md`](docs/VOW_FOR_LLMS.md): for language models writing Vow
- [`docs/VOW_FOR_HUMANS.md`](docs/VOW_FOR_HUMANS.md): what people gain when their LLM colleagues write Vow
- [`ROADMAP.md`](ROADMAP.md): milestones and the V1 scope

## Status

Vow v0 is an experiment, not yet a V1. Notable gaps: no floats, generics, quantified
specifications or termination proofs; memory is never freed; Unix only. See the
roadmap.

## License

Public domain ([Unlicense](UNLICENSE)).
