# Vow roadmap

- [x] M0: Language spec (`docs/SPEC.md`)
- [x] M1: Bootstrap compiler in Rust: lexer, parser, type and effect checker
- [x] M2: C code generation and runtime; example programs run
- [x] M3: Z3 verifier: refinements, contracts, invariants, holes, counterexamples
- [x] M4: Content-addressed hashing and verification cache
- [x] M5: Self-hosted compiler written in Vow, verified by the bootstrap compiler
- [x] M6: Fixpoint: the self-hosted compiler compiles itself to identical C
- [x] M7: `docs/VOW_FOR_LLMS.md`: a guide for other LLMs using Vow in their projects
      (syntax, gotchas, proof workflow, reading verifier output, idioms); every example
      in it is a compiled, verified file under `examples/`
- [x] M7b: `docs/VOW_FOR_HUMANS.md`: how people benefit when their LLM colleagues write
      Vow (reviewing contracts instead of bodies, effects as an audit surface,
      counterexamples, holes as handoff points) and, honestly, what is not guaranteed
- [x] M8: Public GitHub repo (Unlicense), CI on macOS and Linux (Windows waits on M9)
- [x] M9a: Distribution for macOS and Linux: prebuilt binaries (macOS universal, Linux
      x86-64 and arm64, static musl) attached to each release by the Release workflow,
      `install.sh` (served at vow.kranzky.com/install.sh) and a Homebrew tap
      (`kranzky/homebrew-vow`, builds from the C seed and depends on Z3)
- [ ] M9b: Windows: port `run()` to CreateProcess and replace the compiler's use of
      `mktemp`/`rm -rf` with built-ins; MSVC and `zig cc` as C backends; Scoop/winget
- [x] M10: vow.kranzky.com: static site with the spec, the LLM guide, `llms.txt` and
      examples (GitHub Pages + CNAME)
- [ ] M11: Backends beyond C, in order: `#line` directives so C-level errors and
      debuggers point at `.vow` lines (done); bundled `zig cc` to remove the C toolchain
      dependency; a WebAssembly backend (powers a browser playground on the site);
      QBE only if compile speed becomes the bottleneck; a native backend last, if ever
- [x] M12 (built by OpenAI Codex from the docs alone, without the compiler source; brief
      in `docs/tasks/M12-vowsite.md`, report in `tools/vowsite/REPORT.md`): `vowsite`, a static site generator written in Vow that
      builds vow.kranzky.com from a defined Markdown subset. `uses fs` only; proofs
      for bounds, HTML escaping and balanced tags. Runner-up once floats exist: a JSON
      toolkit tested against JSONTestSuite

## V1 scope (after the milestones above)

Must-have, language and proofs:
- [x] Soundness hardening: a written soundness argument (`docs/SOUNDNESS.md`, covering
      the verifier and what it relies on the type checker for), a must-fail suite,
      runtime-checked contracts as the oracle (`--check-contracts`, recursive calls'
      measures included), mutation fuzzing (`tests/fuzz.py`), an independent review of
      the verifier against `SOUNDNESS.md` (after v0.9.2: six bugs found and fixed), and
      a random program generator (`tests/genfuzz.py`: one more bug found and fixed)
- [x] Soundness follow-ups from the review after v0.9.2 (none was unsound): field
      functions in the SMT get unambiguous names (the shared name made Z3 reject some
      correct programs); a measure may not call a function of its own recursive group;
      the cache key covers structural measures
- [x] A second independent review: the type checker against "What the checker
      guarantees" in `docs/SOUNDNESS.md` (after v0.9.6: two unsound bugs, a code
      generation bug in checked builds and five crashes or wrong errors, all fixed)
- [x] Quantified specs (`forall`/`exists` over ranges) and expression functions in specs
- [x] `old(x)` for inout postconditions and loop invariants
- [x] Specification functions with `let`/`if`/`match` bodies, recursive ones with
      measures (defined by axioms, unfolded twice per use)
- [x] Termination measures (`decreases`) for while loops and direct recursion (opt-in)
- [x] Termination for mutual recursion and structural measures (`decreases t` on trees
      and lists); `vowc check --totality` reports which functions are proved to terminate
- [x] Generics: functions, structs and enums; type arguments inferred; each generic
      function verified once for all types and compiled per type
- [x] Float (IEEE doubles), with documented verification semantics: order and NaN-ness
      are known, arithmetic only through what the program states
- [x] Keywords as labels (fields, argument labels, field access, pattern fields)
- [x] `let _ =` discards; `value_or`, `is_some`, `is_none`
- [x] `Result<T, E>` and `try` (also for `Option`)
- [x] Structural `==` for structs, enums, lists and options, and on type parameters
- [x] Modules: `import "m.vow" as m` (qualified names), `import "m.vow" (a, b)`, and
      `private` declarations
- [x] Packages: dependencies pinned to git commits in `vow.pkg`, fetched by `vowc fetch`
- [x] Field refinements assumed inside specifications: a goal assumes the field
      refinements of the struct values it names
- [x] Generic functions usable in specifications: `contains(xs: xs, x: x)` in a
      contract, at concrete types or another generic function's type parameters; each
      instance is translated from its own copy of the body
- [x] Refined type arguments (`List<Cents>`, `Option<Int where it >= 0>`,
      `Result<Cents, String>`, `Box<Nat>`): checked wherever a value is stored, assumed
      wherever it is read; only refinements that mention no variables
- [x] Refined type arguments through generic functions: `last(xs)` on a `List<Nat>`
      is a `Nat`, because a generic function can only return a `T` it was given; not
      where `T` appears in a map or a recursive type
- [x] Lexicographic measures (`decreases a, b`), for recursion that one Int or
      structural measure cannot order: Ackermann's function, nested counters in one
      loop, a rose tree walked through a helper over its children (`examples/rose.vow`)
- [x] An `Eq` bound on type parameters (`fn index_of<T: Eq>(...)`) for types that hold
      no float and no map, required for `==` on `T`: checked at each call, and `==` on
      `T` is an equivalence for the verifier (reflexive, symmetric, transitive)
- [x] An `Ord` bound (`fn sort<T: Ord>(...)`) for `Int`, `String` and type parameters
      declared `Ord`: `<` on `T` is a strict total order for the verifier, and a
      function's type parameter used as a map key needs it; `<` on strings
- [x] Bounds on generic structs and enums (`struct Range<T: Ord>`): field refinements
      may compare bounded parameters, map keys are checked in the declaration, and every
      use of the type is checked against the bounds
- [ ] Lexicographic `Ord` on lists, options and structs (wait for a program that needs
      it: the verifier would need quantified axioms relating a list's order to its
      elements)
- [x] Diagnostics name a module's declarations by the alias the importing file chose
      (`cash.Amount` for money.vow imported `as cash`), or unqualified where the file
      merges or selects them
- [x] Field refinements of list elements in specifications (`xs[i].count` inside a
      `forall`)
- [x] The verifier version (part of every cache key) changes whenever the SMT it
      emits does; `tests/fingerprint.sh` enforces it

Must-have, runtime and portability:
- [x] Memory reclamation: reference counting, with in-place updates of uniquely
      referenced values (as in Koka's Perceus, Lean 4 and Swift's copy-on-write), and
      `set_at`; fannkuch-redux went from 4.34 s and 19.7 GB to 0.39 s and 1 MB
- [x] `HashMap` alongside the persistent ordered `Map`, tree inserts in place, and
      fields moved out of structs on update; k-nucleotide went from 1.11 s to 0.21 s
- [x] `#line` mapping; panics name the `.vow` file and line
- [ ] Windows `run()` (part of M9b; the port rewrites `vrun`, where task A6 is)
- [x] `std/text.vow` (verified text library), `make_dir`, `list_dir`
- [ ] More standard modules and a C FFI; `net`/`time`/`random` effects

Must-have, tooling:
- [x] `vowc fmt` (whitespace only; every file in the repository is formatted, and CI checks it)
- [x] `vowc test`: verifies, then runs each `test_...` function in its own process
- [x] Language server (`vow-lsp`): diagnostics as you edit, proofs on save, hover with
      types and contracts, go to definition
- [x] Language server: hover with the facts the verifier knows at that point
- [x] Pinned Z3 version (5.3.2, installed by `tools/install-z3.sh`; CI uses it)
- [ ] Verifier performance: the driver already uses one Z3 process with push/pop per
      function and obligation; the measured gains are tasks V1, V2, V5 and V6 below
- [ ] Publish a first package: a git repository of verified Vow code, listed at
      vow.kranzky.com/packages.html and imported by a test program through `vow.pkg`

Review after v0.9.9 (five independent reviews of the runtime, code generation, the
verifier pipeline, the front end and the tooling; each task's evidence, a way to
reproduce it and a fix are under the same identifier in
`docs/tasks/review-v0.9.9.md`). No wrong proof was found in normal use.

Wrong behaviour in programs the compiler accepts (first):
- [x] A1: `read_file` on a pipe or other unseekable file overflowed the heap, and on a
      directory returned an empty string; it reads to the end of the stream, and a
      directory gives `None`
- [x] A2: `set xs = xs ++ xs` on a list never terminated
- [x] A3: a struct or variant with 64 or more fields segfaulted, or panicked the
      compiler; the limit is gone (65,534 fields, reported beyond that)
- [x] A4: `==` on a deep value (a million-link chain) overflowed the C stack; the
      generated equality and rank functions no longer recurse
- [x] A5: stack overflow from deep recursion was a segfault; it is a panic ("stack
      overflow (recursion too deep)"), documented as a way a verified program can fail
- [x] A6: `run()` reported success when `fork` failed; allocations in the I/O
      built-ins were unchecked
- [x] A7: write errors on standard output were ignored and the program exited 0; they
      are a panic
- [x] A8: a proof cached under `VOW_SMT_V03=1` was reused by a normal run, which then
      accepted a program it must reject; such a run no longer uses the cache
- [x] A9: `fn main<T>()` passed `check` and failed in the C compiler
- [x] A10: `vowc build prog` overwrote a source file that has no extension

Compiler robustness:
- [x] B1: 25,000 nested parentheses or terms of one `++` chain segfault the compiler;
      limit depth in the parser and iterate over operator chains
- [x] B2: lexer: a newline inside a character literal (wrong line numbers, and `fmt`
      fails), a byte order mark, non-ASCII bytes that make `--json` output invalid,
      `0x1F` and `1e5` split into two tokens, float literals out of range, the minimum
      integer literal
- [x] B3: a directory is read as an empty source file; one file reached by two names
      (case, symlink, absolute path) is loaded twice
- [x] B4: `compute_inline` is cubic: 2,000 one-line functions take 5 s to check, 10,000
      over a minute
- [x] B5: other costs that grow faster than the input: local lookup, type equality by
      rendering, alias resolution, cache keys and lookup; `check` generates C it discards
- [x] B6: generated C: an `if` with over 2,000 `elif` arms does not compile; many `try`
      statements in one function are quadratic; a panic in an `elif` condition names
      the wrong line
- [x] B7: misleading diagnostics: a field named `end`, a local named like a module
      alias, internal module tags, "needs Result, but this is Result", a duplicate
- [x] B8: command line: an unknown command is reported only after the whole build;
      options before the file are rejected

Verifier time and stability (the compiler checks itself in 34 s; with V1, under 1 s):
- [x] V1: functions with nothing to prove are not sent to Z3: 90% of the time was
      three sanity checks that timed out (the compiler checks itself in 1.9 s, from
      33.5 s). Sanity checks still use model-based instantiation: without it every one
      answers `unknown`, for 0.4 s more
- [x] V2: run chunks of functions in parallel Z3 processes (measured at 0.82 s to
      0.24 s for Z3 alone), and ask for the model in the same run
- [x] V3: an overall time limit: a hung or overrunning Z3 hangs `vowc`, and a function
      with k hard obligations costs k times 10 s
- [x] V4: bound queries by Z3's resource count, not wall-clock time, so that an answer
      does not depend on machine load or on what ran before
- [x] V5: cache keys cover only the types and specification functions a function can
      reach: today, editing any one-line function re-verifies the whole program
- [x] V6: assert each fact once per path (script size is quadratic in a path's facts)
- [x] V7: a function that mentions a Float never gets a counterexample, and blames the
      Z3 version
- [x] V8: counterexamples show `old(x)`, and say when a state is a loop's summary
- [x] V9: with `--json`, report solver trouble as a JSON diagnostic naming the function

Checked builds (`--check-contracts`, the fuzzers' oracle):
- [x] K1: a contract that holds can panic with an overflow, because it was proved over
      all integers and is evaluated in 64 bits
- [x] K2: an in-place update through an `inout` parameter copied every time in a checked
      build (80,000 inserts: 21 s against 0.006 s); entry values are captured only
      where `old` is used
- [x] K3: whole values are re-checked at every step (each element's refinement after
      every push; a structural measure's height at every call)

Speed of the runtime and generated code:
- [x] P1: string `++` in place for `set s = s ++ x` (proved string builders, and
      `std/text`'s `to_lower` and `to_upper`, were quadratic), and one allocation for a
      chain of `++`
- [ ] P2: the documented "remove the last element" idiom copies the list; share the
      buffer and truncate in place
- [ ] P3: updating one element of a list of structs allocates a struct
- [ ] P4: borrow the subject of a `match` or `for x in xs` that cannot be reassigned
      (binarytrees: 0.97 s to 0.78 s)
- [ ] P5: `value_or(opt: lookup(...), default: d)` without allocating an Option
- [ ] P6: shared constants for variants without fields; an 8-byte object header with
      per-type descriptors (which also removes the limit in A3)
- [ ] P7: `float_to_str` without 17 print-and-parse round trips

Language server:
- [ ] L1: checking one file clears the diagnostics of every other open file
- [ ] L2: an ordinary function named `test_...` is reported as an error, and hides
      every proof failure in the file
- [ ] L3: positions in UTF-16 code units, not bytes; never negative
- [ ] L4: keep proof failures on screen after the first keystroke that follows a save
- [ ] L5: hover and go-to-definition resolve local names before global ones
- [ ] L6: drop stale edits and prove in a process that a newer edit can abandon (a
      hover waits 23 s behind a cold proof today)

Standard library:
- [ ] S1: `std/json`: surrogates, leading zeros, control bytes in strings, the minimum
      integer, error offsets
- [ ] S2: `std/json`: a depth limit (100,000 nested brackets segfault, and one such
      message kills the language server)
- [ ] S3: `std/text`: contracts for `contains`, `compare`, `replace` and `repeat`

Packages and running programs:
- [x] G1: `vowc add lib <url> main` can pin the branch `feature/main`
- [x] G2: packages: a pin to a missing commit is written before it fails; relative
      URLs; `--` before URLs passed to git; `@lib/../` escapes the package
- [x] G3: `vowc run` passes stdin through and streams output (the program got empty
      input, and its output appeared when it exited), with the new built-in
      `run_attached`

Tests, CI and releases:
- [x] T1: `tests/rc.sh` counts a build that fails as a failure
- [x] T2: harnesses that can pass without checking: the SMT fingerprint after any
      edit to `std/`, the differential test when both compilers fail, the generator
      fuzzer when nothing verifies, the formatting check on a crash; a verified program
      killed by a signal is a finding; timeouts on every CI job
- [x] T3: the Release workflow runs the tests with the binaries before it uploads them,
      and publishes a draft release last; permissions, timeouts and pinned actions in
      CI; the site deploys after CI passes
- [ ] T4: `install.sh` runs the new binary before it replaces the old one, and
      replaces it atomically
- [ ] T5: the Makefile deletes a half-written generated file, and `make test` notices
      stale embedded copies of `std/` and the runtime

Release criteria:
- [ ] Agent benchmark for 1.0: Astra (OpenAI's most capable coding model) builds the
      same project in Vow, Python, Ruby, JavaScript, TypeScript, Go, Rust, C and Dafny,
      several runs each, every one judged by one shared test suite (the project is a
      command-line program, so the tests do not depend on the language) and profiled
      the same way; compare correctness (tests passed, defects found later), effort
      (turns, tokens, wall time, failed compiles and proofs) and performance (time and
      memory). Dafny is the other verified language, so it tests Vow's main claim;
      TypeScript against JavaScript shows what static types alone are worth

- [ ] Stability promise: the spec frozen for 1.x, a versioning and compatibility
      policy, and a decision on whether the frozen Rust bootstrap stays in the test
      suite permanently

Process:
- [x] The Rust bootstrap compiler is frozen at the v0.3 language. It still runs every
      v0.3 program in the test suite, and its verifier must emit the same SMT as the
      self-hosted one for each; new features are built only in Vow

Post-V1: closures, concurrency, Unicode strings, bitwise/fixed-width ints, Wasm/native backends.
