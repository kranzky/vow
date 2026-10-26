# Changelog

## v0.4.0 (unreleased)

Generics and errors as values.

- Generic functions, structs and enums: `fn last<T>(xs: List<T>) -> T`,
  `struct Pair<A, B>`, `enum Tree<T>`. Type arguments are inferred from arguments and
  from the expected type; they are never written at calls.
- Each generic function is type-checked and verified once, with its type parameters as
  unknown types, so its contracts hold for every type it is used with. Each use with
  concrete types is compiled to its own copy of the code.
- `Result<T, E>` (`Ok(value: T)`, `Err(error: E)`) is declared in every program.
- `try`: `let x = try f(...)` returns an `Err` (or `None`) from the enclosing function,
  or binds the value. It is shorthand for a `match`, which is what the verifier sees.
- `try` is a keyword.
- The Rust bootstrap compiler is frozen at the v0.3 language. `make bootstrap` now
  starts from the C seed; the Rust compiler still runs every v0.3 program in the test
  suite, and both verifiers must still emit identical SMT for each of them.
- New example: `examples/results.vow`.

## v0.3.0

Faster maps.

- `HashMap<K, V>`: the same operations, values and verification as `Map`, stored in an
  open-addressing hash table. `keys` is sorted for both. k-nucleotide went from
  1.11 s to 0.21 s, faster than JavaScript and Python.
- `Map` inserts into an unshared tree now rebalance in place instead of rebuilding the
  path to the key.
- In `set s = s with (f: op(s.f, ...))`, the field is moved out of `s`, so lists and
  maps held in structs are updated in place instead of copied on every update.
- New tests for large maps, hash maps, and shared values held in struct fields.

## v0.2.0

Memory management: Vow now frees memory and updates values in place.

### Runtime and code generation

- Reference counting. Every heap object carries a count and a layout record; objects
  are freed as soon as nothing refers to them, iteratively, so long chains cannot
  overflow the stack. A size-class allocator reuses freed memory.
- In-place updates. When a value has a single reference, `with`, `push`, `set_at` and
  `insert` change it in place instead of copying, which preserves value semantics. In
  `set x = op(x, ...)` the old value is moved into `op`, so the common update pattern
  allocates nothing.
- Benchmarks: fannkuch-redux from 4.34 s and 19.7 GB to 0.39 s and 1 MB;
  binary-trees from 2.1 GB to 49 MB (still faster than C); k-nucleotide from 1.6 GB to
  21 MB. Compiling the compiler peaks at 17 MB instead of 92 MB.
- The self-hosted compiler emits the new code. The Rust bootstrap keeps the v0 runtime
  (`bootstrap/vow_rt0.h`) and serves as the first bootstrap stage and as an independent
  front end and verifier: both still emit identical SMT for every program
  (`make differential`).

### Language

- `set_at(list: xs, at: i, value: v)`: a list with one element replaced, with bounds
  and element-level facts for the verifier.
- A call that passes a var `inout` may no longer modify it in another argument
  (`f(x: inout n, by: g(inout n))`); the verifier's model and the runtime disagreed
  there.

### Testing

- `make test-rc` builds every test, example and benchmark, and the compiler itself,
  with a debug runtime that poisons freed memory and panics on any use after free, and
  requires zero objects live at exit.
- A new aliasing test shares values and then updates them through every operation and
  control path; its output must match copying semantics.
- `VOW_CFLAGS` passes extra flags to the C compiler.

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
  or use large maps are slow and memory-hungry (see `bench/RESULTS.md`). Fixed in
  v0.2.0.
- Termination is proved only where a measure is given; integer overflow traps at
  runtime rather than being proved absent.
- macOS and Linux only. Verification requires Z3.
