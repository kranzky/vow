# Vow roadmap

- [x] M0: Language spec (`docs/SPEC.md`)
- [x] M1: Bootstrap compiler in Rust: lexer, parser, type and effect checker
- [x] M2: C code generation and runtime; example programs run
- [ ] M3: Z3 verifier: refinements, contracts, invariants, holes, counterexamples
- [ ] M4: Content-addressed hashing and verification cache
- [ ] M5: Self-hosted compiler written in Vow, verified by the bootstrap compiler
- [ ] M6: Fixpoint: the self-hosted compiler compiles itself to identical C
- [ ] M7: `docs/VOW_FOR_LLMS.md`: a guide for other LLMs using Vow in their projects
      (syntax, gotchas, proof workflow, reading verifier output, idioms); every example
      in it is a compiled, verified file under `examples/`
- [ ] M7b: `docs/VOW_FOR_HUMANS.md`: how people benefit when their LLM colleagues write
      Vow (reviewing contracts instead of bodies, effects as an audit surface,
      counterexamples, holes as handoff points) and, honestly, what is not guaranteed
- [ ] M8: Public GitHub repo (Unlicense), CI on macOS, Linux and Windows
- [ ] M9: Distribution: ship the self-hosted compiler's generated `vowc.c` as the
      bootstrap seed (no Rust needed); prebuilt binaries for macOS arm64/x64, Linux
      x64/arm64 (static musl) and Windows x64 via GitHub Releases, Homebrew tap,
      install script and Scoop/winget; bundle Z3 (MIT); port `run()` to CreateProcess
      on Windows; support `cc`, `clang`, MSVC and `zig cc` as the C backend
- [ ] M10: vow.kranzky.com: static site with the spec, the LLM guide, `llms.txt` and
      examples (GitHub Pages + CNAME)
