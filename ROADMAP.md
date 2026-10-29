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
- [ ] Soundness hardening: written soundness argument (a first version is
      `docs/SOUNDNESS.md`), large must-fail suite, mutation
      fuzzing with runtime-checked contracts as the oracle (`--check-contracts` and
      `tests/fuzz.py` exist; a mutant that verifies and then fails a check is a verifier
      bug), an independent review of the verifier against `SOUNDNESS.md` (done once,
      after v0.9.2: six bugs found and fixed), a random program generator
      (`tests/genfuzz.py`: one more bug found and fixed), runtime checks of recursive
      calls' measures (done), and `SOUNDNESS.md` extended to the type checker
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
- [ ] Generic functions usable in specifications
- [ ] Refined type arguments (`List<Cents>`, `Option<Int where it >= 0>`), so contracts
      can describe what is inside a `List`, `Option` or `Result`: a value is checked
      against the refinement where it goes in (a literal, `push`, `Some(...)`, any
      argument of type `T` to a generic function) and the refinement is assumed where
      it comes out (an index, a loop variable, a `case` binding, a result of type `T`).
      Generic functions are verified for every `T`, so they cannot break it. Only
      refinements that mention no variables, to start with. Today the workaround is a
      struct with a refined field (`examples/money/money.vow`)
- [ ] Lexicographic measures (`decreases (a, b)`), for recursion that one Int or
      structural measure cannot order: a rose tree walked through a helper over its
      children, or mutual recursion mixing a list index and a tree
- [ ] An `Eq` bound on type parameters (`fn index_of<T: Eq>(...)`) for types that hold
      no float and no map. Today a generic function that uses `==` on `T` is rejected
      only at a use with a map type, and its proofs cannot assume `x == x` in case `T`
      is a float (a NaN is not equal to itself); the bound would state the requirement
      in the signature and give back reflexive equality
- [ ] Decide how diagnostics name a module's declarations: by the file's name, as now
      (`money.Amount` even when imported `as cash`), or by the alias the importing file
      chose
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
- [ ] Windows `run()`
- [x] `std/text.vow` (verified text library), `make_dir`, `list_dir`
- [ ] More standard modules and a C FFI; `net`/`time`/`random` effects

Must-have, tooling:
- [x] `vowc fmt` (whitespace only; every file in the repository is formatted, and CI checks it)
- [x] `vowc test`: verifies, then runs each `test_...` function in its own process
- [x] Language server (`vow-lsp`): diagnostics as you edit, proofs on save, hover with
      types and contracts, go to definition
- [x] Language server: hover with the facts the verifier knows at that point
- [x] Pinned Z3 version (5.3.2, installed by `tools/install-z3.sh`; CI uses it)
- [ ] Verifier performance (incremental solving)

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
