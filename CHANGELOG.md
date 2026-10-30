# Changelog

## v0.9.6

- Refined type arguments: `List<Nat>`, `Option<Int where it >= 0>`, `List<List<Nat>>`,
  and refined arguments of generic types (`Result<Nat, String>`, `Box<Nat>`, whose
  fields of type `T` are then `Nat`s). Such a refinement is proved wherever a value is
  stored where it is declared, as one quantified obligation per element refinement
  ("cannot prove every element of 'xs' satisfies it >= 0"), and known wherever a value
  is read: `xs[i]`, `for x in xs`, `case Some(value: v)`, `try`, a field. Checked builds
  check it too. Map values cannot be refined, nor the arguments of a recursive generic
  type (`Tree<Nat>`, an error that says so). Before, any refinement inside a type
  argument was an error, and the workaround was a struct with a refined field.
- Refinements pass through generic functions: with `xs: List<Nat>`, `let n: Nat =
  last(xs)` and `let ys: List<Nat> = reversed(xs)` verify for generic `last` and
  `reversed`. A generic function can only
  return a `T` it was given, so at each call the verifier assumes that if every `T` in
  the arguments satisfies a refinement their declared types give `T`, every `T` in the
  result and in inout arguments does too. It adds facts and no obligations, so no
  program that verified before is rejected. Not where `T` is inside a map in the
  signature, or for refinements that name other variables.
- Inside type arguments, an inline refinement ends at `>`: a comparison with `>` needs
  parentheses (`List<Int where (it > 0)>`), and the error says so.
- `vowc fmt` keeps a `>` inside parentheses in type arguments a comparison.
- `tests/genfuzz.py` generates refined list and option types; with the check of inner
  refinements removed from the verifier, 400 programs give 11 findings. It also
  generates calls to generic functions whose results are bound to refined types; with
  the map exclusion removed, or the implication made unconditional, 300 programs give a
  finding each.
- Verifier version `vow-verifier-11`: calls to generic functions with refined arguments
  give new facts.

- `--check-contracts` checks the measures of recursive functions: at every call within a
  group of recursive functions, the caller's measure on entry is non-negative and the
  callee's, on the arguments, smaller ("recursive call to 'f' did not decrease its
  measure: n"). Structural measures, of loops too, are compared by the value's height.
- A function's `decreases` measure must be defined on entry (its indexes in range and so
  on, given the preconditions), since a checked build evaluates it there. The verifier
  version is now 10.
- `tests/genfuzz.py` writes recursive calls whose argument may not decrease the measure,
  and finds a verifier that skips the measure obligation within a few hundred programs.
- `docs/SOUNDNESS.md` now sets out what the verifier and the code generator rely on the
  type checker for (types, refinements only where checked, names, mutation, effects,
  control flow, literals). Each guarantee was probed with programs that break it; the
  new must-fail tests pin the ones the verifier depends on most.

## v0.9.4

- Fixed (unsound): the axioms of a recursive specification function with a struct
  parameter held of every value, though its body was proved only for values satisfying
  the struct's field refinements. For a struct that no value can satisfy, the
  postconditions were proved vacuously and the axioms were contradictory, so a
  specification naming the function proved anything. The axioms now hold only of real
  arguments. The verifier version is now 9.
- `tests/genfuzz.py` (`make fuzz-gen`): soundness fuzzing with randomly generated
  programs. It found the bug above, and finds four of the six bug classes of v0.9.2.

## v0.9.3

Soundness: an independent review of the verifier against `docs/SOUNDNESS.md` found six
bugs, all fixed. The verifier version is now 8, so results cached by earlier versions
are proved again.

- Fixed (unsound): a `Float` literal whose exponent has leading zeros was misread:
  `1.0e-0000001` (0.1) was taken for 10^-1000000, which let false assertions, and an
  out-of-bounds index, verify. The exponent's value is now used.
- Fixed (unsound): a variable passed inout in a `while` condition was not seen to change
  at the loop head, so assertions after the loop and termination proofs could be wrong.
  An inout argument in a `while` condition is now an error, as in `forall`; the compiler
  had one such loop, now rewritten.
- Fixed (unsound): a struct or variant built on one path was described everywhere, so a
  recursive specification function's axioms (true only of real values) could prove
  anything about a value built in a branch that did not run. Constructions are now
  facts of their path.
- Fixed (unsound): a call in the refinement of a type alias, or in a field refinement of
  a type a function handles, did not link the two functions in the call graph, so two
  functions could prove their postconditions from each other's.
- Fixed (checked builds): specifications are now proved defined (indexes in range,
  `Int` divisors not zero, callees' preconditions met) wherever they are checked; before,
  `assert xs[5] == xs[5]` verified and then failed under `--check-contracts`. The error
  is "cannot prove '...' is defined".
- Fixed (checked builds): a postcondition calling its own function recursed forever
  under `--check-contracts`. Functions called while a specification is checked no longer
  check their own contracts.
- `docs/SOUNDNESS.md` now also covers what the review found undocumented: path facts for
  constructions, the quantified facts and why they are consistent, distinct match
  patterns, effect-free loop conditions and well-defined specifications.
- `tests/fuzz.py` kills a mutant's whole process group on a timeout, so a looping mutant
  run by `vowc run` no longer outlives it.

## v0.9.2

Soundness: two more verifier bugs fixed, runtime-checked contracts, fuzzing, and a
written soundness argument.

- `docs/SOUNDNESS.md`: the argument that the verifier is sound, rule by rule, with what
  it trusts, how it is tested and the soundness bugs found so far.
- Fixed (unsound cache): a function's cached proof was reused after another function
  started calling it back, forming a recursive group with it. Calls within a group must
  decrease the measure, so the proof no longer applied: `vowc check --totality` could
  report as terminating two functions that call each other forever. A function's cache
  key now includes its recursive group. `tests/cache/` checks such edits.
- Fixed (unsound): a variable passed inout on the right of `and`, `or` or `implies`, as
  in `ok = (x > 0) and bump(y: inout y)`, kept its old value as far as the verifier knew,
  so a function could be proved to return 0 and return 100. The verifier now knows the
  variable changes exactly when that side is evaluated. An inout argument inside a
  `forall` or `exists`, whose body runs any number of times, is now an error. The
  verifier version is now 7, so results cached by earlier versions are proved again.
- Fixed: a program could not declare a type named `T` or `E` ("type parameter 'E' has
  the same name as a type", reported in `<std>/prelude.vow`). The prelude now names
  `Result`'s type parameters so that they avoid the program's own type names.
- `--check-contracts` (with `build`, `run` and `test`) checks at runtime everything the
  verifier proves: preconditions, postconditions (with `old`), refinements of
  parameters, results, variables and fields, loop invariants, `Int` loop measures,
  `assert`, and bounds and division. On a verified program a failed check is a bug in
  the verifier; with `--no-verify` it finds the contract that does not hold. The test
  suite runs every test program, example, benchmark and the compiler itself this way,
  under the debug runtime.
- `tests/fuzz.py` (`make fuzz`): soundness fuzzing. It mutates the verified test programs
  and examples (one literal, comparison or operator each) and runs every mutant that
  still verifies with `--check-contracts`; a check that fails is a verifier bug. 6,000
  mutants found none; with the check that loop invariants are preserved disabled, 400
  mutants found it 7 times. `--target compiler` (`make fuzz-compiler`) mutates the compiler's
  own source and has each mutant that verifies, built with checks, verify and compile
  test programs; 400 mutants found nothing.

## v0.9.1

Soundness fixes.

- Fixed (unsound): the refinement of a `var` or an `inout` parameter could name another
  variable that changes, such as `var y: Int where it > x`. The refinement was checked
  only when its own variable was assigned, but assumed again at loop heads and after
  calls, so a program could prove a false postcondition. Such refinements are now
  rejected: they may name only `let`s and parameters that are not `inout`.

## v0.9.0

Modules and packages.

- `import "geo.vow" as geo` imports a file as a module: its public declarations are
  named `geo.dist(...)`, `geo.Point`, `geo.Shape.Box(...)`, and nothing else it imports
  comes along. `import "std/text.vow" (split, trim)` brings in just those names.
  A plain `import "file.vow"` still merges the file, as before.
- `private fn` (and `private struct`, `enum`, `type`) keeps a declaration visible only
  in its own file, however the file is imported. A public declaration cannot name a
  private type in its signature, fields or variants.
- `std/json.vow` is meant to be imported as a module, and its functions lose their
  prefix: `json.parse`, `json.text`, `json.quote`, `json.get`, `json.get_string`,
  `json.get_int`, `json.string`, `json.int`, `json.float` and `json.hex_digit` (they
  were `json_parse` and so on). The type `Json` is now `json.Value`, and its variants
  lose their `J`: `Null`, `Bool`, `Int` (was `JNum`), `Float`, `String`, `Array` and
  `Object`. Its reader internals are private, and it imports only the three text
  functions it uses.
- Using a name the file cannot see says why: it is private, it is in a module (write
  `geo.origin`), or it is declared in a file that is not imported (with the import
  line that would fix it).
- Packages: a project's `vow.pkg` pins each package to a git commit, and files import
  them as `"@name/file.vow"`. `vowc add <name> <git URL> [ref]` pins one and fetches
  it; `vowc fetch` fetches everything listed, and the packages' own packages, into
  `.vow/pkg/`. Package code is verified with the program that uses it.
- The language server's hover and go-to-definition follow qualified names into
  modules; `vowc fmt` lays out the new forms.
- `examples/prices.vow` uses a module of its own, `examples/money/money.vow`.
- The site has a [Packages](https://vow.kranzky.com/packages.html) page: how to use
  and publish a package, and a list of published ones.
- Programs without modules are unchanged: their declarations keep their names, and
  the verifier emits the same SMT.

## v0.8.0

Floats.

- `Float`: 64-bit IEEE doubles, with literals (`1.5`, `2.5e-3`), `+ - * /`, negation,
  comparisons, and `int_to_float`, `float_to_int`, `float_to_str`, `str_to_float`,
  `floor`, `ceil`, `round`, `sqrt`, `abs_float` and `is_nan`. Floats are unboxed; their
  arithmetic never traps. `Int` and `Float` never mix.
- Verification: a float is known by its order and whether it is a NaN, so comparisons
  reason soundly without axioms; literals, negation, `abs_float`, `floor`, `ceil`,
  `sqrt` and `int_to_float` have their exact or monotone facts; nothing else about
  arithmetic is assumed. Must-fail tests check that `(0.1 + 0.2) == 0.3`, `r == r` for a
  struct that may hold a NaN, and `x + 1.0 > x` are not proved.
- `==` on a type that can hold a float (including a type parameter) no longer assumes
  that a value equals itself, since a NaN does not; generated code drops its
  same-object shortcut for such types (verifier version 6).
- `float_to_str` prints the shortest text that reads back as the same double, as Python
  does (`0.1`, `300.0`, `1e+16`).
- `std/json.vow` reads and writes fractional numbers (`JFloat`), and integers too large
  for an `Int`.
- `float_to_fixed(x, digits)` writes a Float with a fixed number of decimals, as C's
  `printf("%.*f")` does.
- Every Float operation is rounded as written: generated C may not fuse a multiply and
  an add, so results do not depend on the C compiler or the processor.
- `set_at` on a list with a single owner is now an inline store; fannkuch-redux went
  from 0.39 s to 0.24 s.
- Benchmarks: n-body, spectral-norm and mandelbrot in Vow, C, Rust, Go, JavaScript and
  Python (`bench/`); Vow matches C on mandelbrot and takes about 1.6 times as long on
  the other two.
- Fixed: `vowc fmt` did not know Float literals with exponents (it refused to format
  such files rather than change them).

## v0.7.0

Tooling: a formatter, a test runner and a language server; JSON; a pinned Z3.

- `vow-lsp`, a language server (built from `tools/lsp/vowlsp.vow`, shipped in the release
  archives): type and effect errors as you edit, failed proofs and holes when a file is
  opened or saved, hover with a function's contract or an expression's type (and what
  the verifier knows about it at that line), and go to definition. Its test drives a session from a Vow program (`make test-lsp`). The
  README shows how to set it up in Neovim, Helix and Emacs.
- The checking and verification pipeline is now one module (`compiler/pipeline.vow`)
  shared by `vowc` and the language server, and `load_with` checks unsaved buffers.

- `std/json.vow`: a verified JSON parser and printer (integer numbers only, until Vow
  has floats). Its reader's termination is proved with `old(x)` contracts.
- `read_line()` and `read_bytes(n)` read standard input (`uses console`).

- `vowc test file.vow` verifies the program, then runs each function named `test_...`
  in the file, each in its own process: a failed `check` or a panic fails only that
  test, and its output (with the panic's source line) is shown. A file of tests needs
  no `main`; `--json` prints one object per test.

- `vowc fmt file.vow` rewrites a file in the canonical layout. It changes only
  whitespace and writes nothing unless the result has the same tokens and comments;
  `--check` only reports. Every Vow file in the repository is formatted, and the tests
  check that it stays so.
- Faster verification of programs that use structural `==`: each type's equality axiom
  is now stated only in the proofs of functions that compare such values, after the
  check that their definitions are consistent. In the global preamble it made Z3 spend
  its whole timeout on every function's consistency check (verifier version 5).

- Z3 is pinned: Vow is tested with Z3 5.3.2, which `tools/install-z3.sh` installs
  (checking its SHA-256) and CI uses. `vowc version` shows the Z3 it finds, and when a
  proof is left undecided by a different Z3, `vowc` says which version it is tested with.
- Specifications also assume the field refinements of list elements in range:
  `forall k in 0..len(xs): xs[k].count == len(xs[k].values)` is proved for any list
  of structs whose `count` field is refined that way.
- The verifier version, part of every verification cache key, is now `vow-verifier-4`,
  and must change whenever the SMT the verifier emits does: `tests/fingerprint.sh`
  compares a hash of the SMT for every test program and example with the one recorded
  for the current version. Cached results are reused only by the verifier that proved
  them, so a fix to the encoding never trusts results from before it.

## v0.6.0

Termination, recursive specification functions and readable panics.

- Mutual recursion is measured: functions that call each other form a recursive group;
  when one has `decreases`, all must, and every call within the group must decrease
  the measure.
- Structural measures: `decreases t` where `t` is a list, struct, enum or option value
  proves recursion on its parts terminates (tree and rose-tree walks).
- `vowc check --totality` reports which functions are proved to terminate, and for
  the others, why not (an unmeasured loop or recursion, or a call to such a function).
- Specification functions: any pure function whose body uses only `let`, `if`,
  `match`, `assert` and `return` can be used in contracts, and recursive ones too when
  they have a measure: `ensures len(out) == (len(old(out)) + size(t))`. Recursive ones
  are defined by axioms unfolded at most twice per use; their definitions are kept out
  of the proofs of their own recursive group, whose termination they rely on.
- `vowc`, `vowc help` and `vowc --help` point to the guide for LLMs.
- Generated C carries `#line` directives, so C compiler errors and debuggers point at
  `.vow` lines (`--no-line-directives` turns them off; the committed seed uses it).
- Runtime panics name the source line: `vow: panic at calc.vow:12: integer overflow in
  '*'`. This covers overflow, `panic(msg)`, and bounds checks in unverified builds, at
  no cost when nothing fails (the benchmarks are unchanged).
- Fixed: a variant without fields (`Tree.Leaf`) in a specification made Z3 reject the
  query; it now denotes a value with that variant's tag.
- Specifications assume the field refinements of the struct values they name:
  `assert l.count == len(l.values)` is proved for a parameter `l` whose `count` field is
  refined that way. Before, only code that read a field assumed them.

## v0.5.0

Structural equality and `old(x)`.

- `==` and `!=` work on every type without maps: lists element by element, structs
  field by field, enums and options by variant and fields. The verifier can prove two
  values equal however they were built, recursive types included.
- `==` on a type parameter: a generic function such as `index_of<T>` or `dedupe<T>` is
  verified once, knowing nothing about equality on `T`, and each use with a map type
  is rejected, naming the instance.
- A variant whose fields are all `Unit` is written without them: a function returning
  `Result<Unit, E>` ends with `return Result.Ok`.
- `old(x)` in `ensures` and loop invariants names an `inout` parameter's value on
  entry, so a contract can say how the parameter changed:
  `ensures len(out) == (len(old(out)) + 1)`. Callers get the facts about their own var.
- `old` is reserved as a function name.

## v0.4.0

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
