# Changelog

## v0.1.0

The first release of Vow: an experimental programming language designed for language
models to write, with contracts proved by the Z3 SMT solver at compile time.

### The language

- Refinement types (`Int where it >= 0`), `requires`, `ensures`, `assert` and loop
  invariants, all proved by Z3. Index bounds, division by zero and slice ranges are
  proof obligations. Failed proofs report counterexamples.
- Bounded quantifiers (`forall` and `exists` over integer ranges), in specifications
  and at runtime.
- Expression functions: pure single-expression functions can be used in
  specifications and are transparent to the verifier.
- Termination measures (`decreases`) for `while` loops and direct recursion.
- Effects in signatures: `uses console`, `uses fs`, `uses proc`, checked transitively.
- No operator precedence, labelled block ends, named arguments, no shadowing, no null,
  no exceptions, no implicit conversions.
- Immutable values with mutable value semantics: `var`, `set` and `inout` parameters.
- Typed holes (`?name`) that report the expected type, the variables in scope and the
  facts the verifier knows.
- Content-addressed verification cache: a function is re-verified only when its body or
  the contracts of what it calls change.
- `std/text.vow`, a verified standard text library, built into the compiler.

### The compiler

- `vowc`, written in Vow (about 6,000 lines), verifies itself with no runtime-checked
  escape hatches, and compiles itself to byte-identical C (the bootstrap fixpoint).
- `seed/vowc.c` builds the compiler with only a C compiler: `make`.
- The original Rust bootstrap compiler (`bootstrap/`) is kept as an independent second
  implementation; both emit identical C and identical SMT for every test program.
- Commands: `check`, `build`, `run`, `emit-c`, `hash`, `version`; `--json`
  diagnostics.

### Documentation and examples

- [Specification](docs/SPEC.md), [Vow for LLMs](docs/VOW_FOR_LLMS.md),
  [Vow for humans](docs/VOW_FOR_HUMANS.md), [How Vow compares](docs/COMPARISON.md),
  and the website, https://vow.kranzky.com.
- Verified examples: a fully specified binary search, insertion sort proved sorted, an
  HTML escaper proved never to emit `<` or `>`, a recursive-descent calculator proved
  to terminate, and more.
- `tools/vowsite`, the static site generator that builds the website, was written by
  another model (OpenAI Codex) from the documentation alone; see its report.
- `bench/`: three Benchmarks Game programs in Vow, C, Rust, Go, JavaScript and Python.

### Known limitations

- No floating point, generics, modules, closures or concurrency.
- Memory is never freed, and immutable updates copy: programs that update data in place
  or use large maps are slow and memory-hungry (see `bench/RESULTS.md`).
- Termination is proved only where a measure is given; integer overflow traps at
  runtime rather than being proved absent.
- macOS and Linux only. Verification requires Z3.
