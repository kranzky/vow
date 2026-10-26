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
- [ ] M9: Distribution: ship the self-hosted compiler's generated `vowc.c` as the
      bootstrap seed (no Rust needed); prebuilt binaries for macOS arm64/x64, Linux
      x64/arm64 (static musl) and Windows x64 via GitHub Releases, Homebrew tap,
      install script and Scoop/winget; bundle Z3 (MIT); port `run()` to CreateProcess
      on Windows; support `cc`, `clang`, MSVC and `zig cc` as the C backend
- [x] M10: vow.kranzky.com: static site with the spec, the LLM guide, `llms.txt` and
      examples (GitHub Pages + CNAME)
- [ ] M11: Backends beyond C, in order: `#line` directives so C-level errors and
      debuggers point at `.vow` lines; bundled `zig cc` to remove the C toolchain
      dependency; a WebAssembly backend (powers a browser playground on the site);
      QBE only if compile speed becomes the bottleneck; a native backend last, if ever
- [x] M12 (built by OpenAI Codex from the docs alone, without the compiler source; brief
      in `docs/tasks/M12-vowsite.md`, report in `tools/vowsite/REPORT.md`): `vowsite`, a static site generator written in Vow that
      builds vow.kranzky.com from a defined Markdown subset. `uses fs` only; proofs
      for bounds, HTML escaping and balanced tags. Runner-up once floats exist: a JSON
      toolkit tested against JSONTestSuite

## V1 scope (after the milestones above)

Must-have, language and proofs:
- [ ] Soundness hardening: written soundness argument, large must-fail suite, differential
      fuzzing of the two verifiers
- [x] Quantified specs (`forall`/`exists` over ranges) and expression functions in specs
- [ ] `old(x)` for inout postconditions; recursive spec functions (needs termination)
- [x] Termination measures (`decreases`) for while loops and direct recursion (opt-in)
- [ ] Termination for mutual recursion; report which functions are proved total
- [ ] Generics (monomorphised functions and types)
- [ ] Float, with documented verification semantics
- [x] Keywords as labels (fields, argument labels, field access, pattern fields)
- [x] `let _ =` discards; `value_or`, `is_some`, `is_none`
- [ ] Structural `==` for structs/enums; `Result` with error propagation
- [ ] Modules with visibility, then packages

Must-have, runtime and portability:
- [ ] Memory reclamation: reference counting, with in-place updates of uniquely
      referenced values (as in Koka's Perceus, Lean 4 and Swift's copy-on-write); today
      fannkuch-redux allocates 19.7 GB (see `bench/RESULTS.md`)
- [ ] A hash map alongside the persistent ordered `Map`
- [ ] Windows `run()`, `#line` mapping, readable panics
- [x] `std/text.vow` (verified text library), `make_dir`, `list_dir`
- [ ] More standard modules and a C FFI; `net`/`time`/`random` effects

Must-have, tooling:
- [ ] `vowc fmt`, `vowc test`, LSP (hover shows type and known facts)
- [ ] Verifier performance (incremental solving), pinned Z3 version

Post-V1: closures, concurrency, Unicode strings, bitwise/fixed-width ints, Wasm/native backends.
