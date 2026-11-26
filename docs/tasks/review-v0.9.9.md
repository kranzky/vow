# Review after v0.9.9: performance, stability and latent bugs

A review of the whole project at commit 30c4c1d (v0.9.9 plus lexicographic measures),
made by five independent reviewers (the C runtime, code generation, the verification
pipeline, the front end, and the tooling and CI) and then checked: every finding marked
**reproduced** was run again by a second reader; the others are from reading the code
or from one reviewer's measurement, and say so.

Each finding has an identifier, and `ROADMAP.md` has one task for each ("Review after
v0.9.9"). Line numbers are for commit 30c4c1d. Timings are from an Apple Silicon Mac
with Z3 5.3.3, with other work running, so treat them as approximate.

The earlier reviews covered the verifier's logic and the type checker's guarantees.
This one found no wrong proof in normal use. What it found is at the edges: the
runtime, limits nobody enforces, the test harness, and time.

## A. Wrong behaviour in programs the compiler accepts

These break the promise in `docs/VOW_FOR_LLMS.md` that a verified program's only
runtime failures are integer overflow, `panic` and a failed `check`. Fix these first.

### A1. `read_file` overflows the heap on a file that cannot be seeked (reproduced)

- **Where:** `runtime/vow_rt.h:1033-1044` (`vread_file`).
- **What:** `ftell` returns -1 on a pipe, so `(size_t)n + 1` is 0, a 32-byte object is
  allocated, and `fread` reads the whole stream into it.
- **Reproduce:** a verified program that prints `len(s)` for `read_file(args()[0])`,
  run as `./rf <(head -c 3000000 /dev/zero)`: prints `read 3000000 bytes`, then
  `Abort trap: 6`. A short pipe (`<(echo hi)`) corrupts memory silently and exits 0.
- **Same cause:** `read_file` on a directory returns `Some("")` on macOS (reproduced);
  on Linux `ftell` gives `LONG_MAX` there, which would panic with "out of memory"
  (from reading).
- **Fix:** read in a loop into a growing buffer until end of file; reject directories
  with `fstat`.
- **Done:** `vread_file` reads in a loop into a growing buffer and gives `None` for a
  directory or a read error. Test: `tests/run/read_file_stream.vow` (a FIFO).

### A2. `set xs = xs ++ xs` on a list never terminates (reproduced)

- **Where:** `compiler/codegen.vow:492-496` (`move_candidate`, the `++` case) with
  `runtime/vow_rt.h:726-733` (`vlist_cat`).
- **What:** the left `xs` is moved and the right `xs` is a borrowed copy of the same
  pointer, so `vlist_cat(a, b)` has `a == b` with a count of one; each push grows the
  list in place, and the loop bound `VL(b)->len` grows with it.
- **Reproduce:** `var xs = [1, 2, 3]`, `set xs = xs ++ xs`, print `len(xs)`: verifies,
  prints nothing, and uses 6 GB within five seconds. The same through an `inout`
  parameter. `let ys = xs ++ xs` and strings are fine.
- **Fix:** read `VL(b)->len` once before the loop in `vlist_cat` (tried in a copy:
  correct, clean under the sanitizers), or do not move the left operand when the right
  one names the same variable.
- **Done:** `vlist_cat` reads the right operand's length once. Test:
  `tests/run/list_cat_self.vow`.

### A3. A struct or variant with 64 or more fields crashes (reproduced)

- **Where:** `compiler/codegen.vow:1432-1449` (`field_mask`); the 64-bit mask in
  `runtime/vow_rt.h:82`, used at `:277-278` and `:502-503`.
- **What:** the mask bit stops doubling at 2^62, so every field from index 62 on gets
  bit 62. The real limit is 63 fields for a struct and 62 for a variant.
- **Reproduce:** a struct with `f0..f62: Int` and `f63: String`: builds, and the
  program dies with `Segmentation fault: 11`. With 64 `String` fields, `vowc check`
  itself dies: `vow: panic at compiler/codegen.vow:1442: integer overflow in '+'`.
- **Fix:** a diagnostic for types over the limit now; a per-type descriptor in place of
  the per-object mask later (see P6), which removes the limit.
- **Done:** the limit is removed instead of reported. An object of more than 63 words
  has a flag, and its header's mask is a pointer to a static bitmap the compiler emits
  for its type (`vow_fields_wide`, `new_fields` in `compiler/codegen.vow`). The header
  counts words in 16 bits, so more than 65,534 fields is an error. Test:
  `tests/run/wide_types.vow` (200 fields; a variant with 70).

### A4. `==` on a deep value overflows the C stack (reproduced)

- **Where:** `compiler/codegen.vow:1744-1794` (`gen_eq_fn`); `vrank_T` at `:1833-1880`
  recurses the same way.
- **What:** the generated `veq_T` recurses on every field. `vow_destroy` was made
  iterative for this shape; equality was not.
- **Reproduce:** `enum Chain End, Link(head: Int, tail: Chain)`, two chains of
  1,000,000 links built in a loop, then `c1 == c2`: `Segmentation fault: 11`. 100,000
  links work.
- **Fix:** loop on the last heap field (or use an explicit work stack, as
  `vow_destroy` does), for `veq_` and `vrank_`.
- **Done:** each type has a step function that compares one pair of objects and pushes
  the parts that could recurse onto a work stack (`vow_work_run`); other parts are
  compared by a direct call, so equality on ordinary nested structs costs what it did.
  Rank is found the same way, as the greatest depth reached. Test:
  `tests/run/deep_values.vow` (a million links, three shapes).

### A5. Stack overflow is an unreported way for a verified program to fail (reproduced)

- **Where:** no stack guard anywhere in `runtime/vow_rt.h`.
- **What:** a verified `fn count(n) ... decreases n` called with 10,000,000 segfaults.
  So does `std/json.vow`'s parser at 100,000 nested brackets (S2), and the compiler
  itself (B1). The docs list the ways a verified program can fail, and this is not one.
- **Fix:** a `sigaltstack` handler (or a depth counter in checked builds) that prints
  `vow: panic: stack overflow`, and a line in the spec and the guides.
- **Done:** `vow_guard_stack` (called first in `main`) handles SIGSEGV and SIGBUS on a
  stack of its own. A fault at an address within the stack's limit, or the megabyte
  below it, prints `vow: panic: stack overflow (recursion too deep)` and exits 101,
  after flushing standard output unless it was being written. Any other fault ends
  the program as before. The compiler gets the same handler, so B1 is now a panic
  instead of a segfault (still without a diagnostic). Test:
  `tests/panic/stack_overflow.vow`.

### A6. `run()` reports success when `fork` fails (reproduced)

- **Where:** `runtime/vow_rt.h:1164-1190` (`vrun`).
- **What:** `pid == -1` is not handled: the result is `code = 0, out = ""`.
- **Reproduce:** `(ulimit -u 1; exec ./prog)` where the program runs `echo hello`:
  prints `code=0 out=[]`. The compiler trusts `r.code != 0` after running `cc`
  (`compiler/vowc.vow:271-275`), so under a process limit it would report a build that
  produced nothing.
- **Fix:** return code -1 when `fork` fails, and check `waitpid`.
- **Same function, from reading:** the temp file for input is always in `/tmp`
  (ignores `TMPDIR`), and is created even for empty input.
- **Also from reading:** `malloc`/`realloc` results are unchecked at
  `vow_rt.h:1008, 1058, 1081, 1085, 1171, 1180, 1182`; every other allocation panics
  with "out of memory". Route them through one checked helper.
- **Done:** `vrun` gives code -1 when `fork` fails, when the input cannot be staged,
  or when the child did not exit by itself; `read`, `write` and `waitpid` are retried
  after a signal; the temporary file goes in `$TMPDIR` and is not made for empty input
  (`/dev/null` is used); the argument vector is built before the fork. Every
  allocation in the built-ins goes through `vow_xmalloc`/`vow_xrealloc`. Test:
  `tests/run/run_codes.vow`; the `fork` case was checked by hand with `ulimit -u 1`.
- **Still open, seen while testing:** with standard output closed, `pipe` can hand
  `run()` descriptor 1, and the child's output is lost.

### A7. Write errors on standard output are ignored (reproduced by one reviewer)

- **Where:** `runtime/vow_rt.h:1022-1025` (`vprint`), `:1138-1141` (`vexit`), and the
  emitted `main`.
- **What:** a program whose every write fails (`./prog >&-`, or a full disk) exits 0.
- **Fix:** check `fflush` and `ferror(stdout)` at exit and panic with a write error.
- **Done:** `print` panics when its write fails, and the end of `main` and `exit` panic
  when the flush fails: `vow: panic: cannot write to standard output`, status 101.
  `tests/run.sh` checks it with `>&-`.

### A8. A proof cached under `VOW_SMT_V03=1` is reused by a normal run (reproduced)

- **Where:** `compiler/pipeline.vow:95-98`, `compiler/hash.vow:216`,
  `compiler/vowc.vow:396`.
- **What:** the switch (used by `tests/differential.sh`) turns off part of the encoding
  but is not in the cache key and does not turn the cache off.
- **Reproduce:** on a copy of `tests/fail/unreal_struct.vow`: `vowc check` says
  "cannot prove assertion: x > 0"; `VOW_SMT_V03=1 vowc check` says `ok`; after that,
  plain `vowc check` says `ok` too, and the program panics under `--check-contracts`.
  The test suite passes `--no-cache`, so it does not do this itself; an exported
  variable would.
- **Fix:** make the switch imply `--no-cache`, or put it in the key.
- **Related, from reading:** the key does not include the Z3 version or `VOW_Z3`. No
  wrong accept was constructed from that.
- **Done:** a run with `VOW_SMT_V03` set neither reads nor writes the cache
  (`verify_source`). `tests/run.sh` checks it with `tests/fail/unreal_struct.vow`. The
  related point (Z3 version and `VOW_Z3` are not in the key) is still open.

### A9. `fn main<T>()` passes `check`, then fails in the C compiler (reproduced)

- **Where:** `compiler/check.vow:1705-1711` checks main's parameters and result, not
  its type parameters.
- **Reproduce:** `fn main<T>() uses console do print("hi\n") end fn main`: `vowc check`
  prints `ok`; `vowc run` prints `call to undeclared function 'f_main'`.
- **Fix:** "'main' must not have type parameters".
- **Done:** "'main' must not have type parameters". Test:
  `tests/fail/main_generic.vow`.

### A10. `vowc build prog` overwrites a source file that has no extension (reproduced)

- **Where:** `compiler/vowc.vow:431` (`basename_stem`).
- **Reproduce:** `cp echo.vow prog; vowc build prog; file prog` says `Mach-O 64-bit
  executable`.
- **Fix:** refuse an output path equal to the input.
- **Done:** `build` refuses an executable path equal to the source path (leading `./`
  ignored), with or without `-o`, and exits 2. `tests/run.sh` checks both and that
  the source is unchanged.

## B. Compiler robustness

About 212,000 generated inputs (token, line and byte mutants of the tests and of the
compiler's own source, token soup, random bytes) produced no crash, panic, internal
error or hang from `check`, `fmt` or `check --json`. The findings below came from
structured inputs.

### B1. Deep nesting or a long operator chain segfaults the compiler (reproduced)

- **Where:** `compiler/parser.vow` (`parse_primary`, `parse_unary`, `parse_postfix`
  recurse); for flat chains, a later pass that recurses over the left-leaning `Binary`
  tree.
- **Reproduce:** `let x = "a" ++ "a" ++ ...` with 30,000 terms:
  `vowc check --no-verify` dies with `Segmentation fault: 11`. First crash at about
  25,000 terms or unary minuses, 29,000 `(`, 33,000 `[`. 40,000 `(` and nothing else
  crashes too.
- **Why it matters:** `tools/embed.vow` emits one `++` term per line (the embedded
  runtime is a 1,200-term chain), so embedding a 25,000-line file crashes the compiler.
- **Fix:** count depth in the parser and report "nested too deeply"; make the passes
  over `Binary` iterate down the left spine.
- **Done, with a limit in place of the iteration:** the parser refuses anything more
  than 1,000 levels deep (`max_depth` in `compiler/parser.vow`). It counts its own
  recursion (expressions and types in `Parser.depth`, blocks in `Parser.blocks`) and
  the height of each expression it builds (`Parser.high`), so a chain, which is a deep
  tree made without recursion, is limited too: its terms are levels. About 27
  functions in the later passes recurse over `Binary`; making each iterate would not
  have removed the limits after them (5,000 `elif` arms already fail in the C
  compiler, B6). Before the limit, the passes overflowed at about 5,000 levels for most
  kinds of nesting (measured on an 8 MB stack); a program 1,000 deep in blocks, in a
  sum and in an `and` chain at once builds and runs. `tools/embed.vow` puts its lines
  in parenthesised groups of 200. Tests: the "1000 levels deep" block in
  `tests/run.sh` (at the limit, one past it, and 40,000 of each kind).
- **Also done (found while testing the above):** a text embedded as one literal per
  line took minutes to build. Two causes, both in the generated C: a statement per
  string literal at start-up (now a table and a loop), and one 30,000-element array
  for the chain (now literals next to each other are joined at compile time, and a
  chain is appended at most 256 strings at a time).

### B2. Lexer: bytes and literals it mishandles (reproduced)

- **A raw newline inside a character literal is accepted** (`compiler/lexer.vow:165`)
  and the line counter is not advanced: later errors are one line too low, and
  `vowc fmt` fails on a program that `check` accepts ("internal error: formatting ...
  would change its tokens").
- **A non-ASCII byte outside a string** gives `unexpected character '?'` with one raw
  byte (`lexer.vow:243`), and with `--json` the output is not valid UTF-8, so a JSON
  parser rejects it (`vowc.vow:42-62`, `json_str`). A UTF-8 byte order mark at the
  start of a file is rejected the same way.
- **Unsupported number forms are split into two tokens:** `0x1F`, `12abc`, `1e5`,
  `1_000` all give "only a call can be used as a statement".
- **Float literals are not range-checked:** `1.5e400` is `inf` at run time.
- **`-9223372036854775808` is rejected** ("integer literal does not fit in 64 bits").
- **A file with CR-only line ends** reports every error on line 1.
- **Fix:** reject control bytes in character literals; skip a leading byte order mark;
  show the whole UTF-8 sequence and escape invalid bytes in JSON; lex a number with its
  trailing letters and report "malformed number"; check float literals; accept the
  minimum integer.
- **Done** (`1_000` was already accepted when this was taken up):
  - a control character in a character literal is an error;
  - a byte order mark at the start is skipped, and `vowc fmt` keeps it;
  - an unexpected character outside ASCII is shown whole if it is UTF-8 and as
    `unexpected byte 0x..` if not (`utf8_len` in `compiler/util.vow`), and `json_str`
    writes any byte that is not UTF-8 as U+FFFD;
  - a number followed by a letter or digit is `malformed number '...'`;
  - a `Float` literal that would be infinity, or `0.0` without being zero, is an error
    (`float_literal_range`; `decimal_of` moved from the verifier to `util.vow`). The
    second kind was unsound: the verifier orders literals by their decimal values, so
    it proved `1.5e-400 > 0.0`;
  - `9223372036854775808` straight after a unary `-` is the smallest `Int`: the lexer
    gives the token that value, and the parser accepts it only there (an expression
    or a pattern). The generated C says `INT64_MIN`;
  - a carriage return with no line feed after it is an error, rather than a line end:
    nothing is written with such line ends now, and the error says what to do.
  Tests: `tests/fail/lex_*.vow`, `float_literal_*.vow`, `int_literal_*.vow`;
  `tests/run/int_min.vow`, `float_limits.vow`, `byte_order_mark.vow`; the `--json`
  case in `tests/run.sh`.

### B3. Files: a directory is an empty source file, and one file can load twice (reproduced)

- **Where:** `compiler/load.vow:20-45, 124, 140`; `format_file` in `vowc.vow`.
- **What:** `import "sub"` where `sub` is a directory is accepted as an empty module
  (the error appears later as "unknown function"); `vowc check .` says "the program
  has no 'fn main()'". A file reached as `lib.vow` and `LIB.vow` (macOS), through a
  symlink, or by an absolute and a relative path is loaded twice: "function 'one' is
  already declared at lib.vow:1", reported inside the library.
- **Fix:** report "cannot open ... (it is a directory)"; identify files by `realpath`.
- **Done.** `read_file` already gave `None` for a directory (R1); the loader now adds
  `: it is a directory` when `list_dir` succeeds on the path. A file is known by its
  real path (`keys` in `load_with`), and another path to a file already loaded is
  recorded in `same` and linked to it, so it is loaded once. That needed a built-in,
  `real_path`, which is newer than the v0.3 language: the compiler calls it through
  `file_identity` in `compiler/attached.vow`, and the stand-in in
  `tests/v03/attached.vow` gives the path back. An import spelled in another case than
  the file is an error (`case_mismatch`), where `realpath` shows the file's own
  spelling: the import would fail on a case-sensitive file system, so it fails
  everywhere. A part that is in its directory as spelled (a link whose name differs
  from its target only in case) is not taken for one. Tests: the "loaded once" block
  in `tests/run.sh`, and `tests/run/real_path.vow`.

### B4. `compute_inline` is cubic in the number of small pure functions (reproduced by one reviewer)

- **Where:** `compiler/check.vow:2160-2267`, `reaches` at `:2558`, `remove_key` at
  `:2278`.
- **What:** one depth-first search per candidate with a list as the visited set; a
  `while changed` loop that drops one call-graph layer per round; a map rebuilt per
  dropped name.
- **Measured (`check --no-verify`):** a chain of 2,000 one-line functions takes 5.3 s
  and 10,000 over 60 s; with a last function that is not specification-safe, 500 take
  10 s and 1,000 over 120 s. Profile: 99% in `reaches`.
- **Fix:** strongly connected components once (as `total.vow` does), then a worklist
  over reverse call edges.
- **Done.** The loop's rounds are replaced by what they compute. A candidate is dropped
  in the first round if its body is not specification-safe or it is on a cycle with no
  measure; after that only "calls a dropped candidate" can drop one (dropping never
  makes a cycle), so that is a worklist over the callers, and recursion is looked for
  twice in all (`on_cycles`, with `components` moved from `total.vow` to `util.vow`).
  `recursion` in `total.vow` looked names up in lists; it uses maps. Measured
  (`check --no-verify`): the chain of 2,000 takes 0.18 s and 10,000 take 0.5 s; 40,000
  take 1.7 s, also when the last is not specification-safe.
- **Found on the way:** popping a stack with `slice` copied the list, which made every
  deep search in the compiler quadratic. `slice` of a list now consumes it, and a
  prefix of an unshared list is made in place (`vlist_cut` in the runtime;
  `consumes_first` in `codegen.vow`). Test: `tests/run/slice_pop.vow`.
- **Not done:** a chain of N functions that each inline the next is expanded in full
  for each of them by the verifier, N²/2 expansions in all: 3,000 take 5.6 s to check.
  Real programs do not have such chains.

### B5. Other front-end costs that grow faster than the input (measured by one reviewer)

The front end is otherwise linear: the compiler's 17,500 lines check in 0.13 s.

- **Local lookup is a linear scan** (`check.vow:1310, 1330`): one function with 40,000
  `let`s takes 2.5 s, and 7.1 s when each binds a call.
- **`ty_eq` compares types by rendering them** (`check.vow:56`): a list literal nested
  4,000 deep takes 5 s.
- **Aliases resolve one per round** (`check.vow:1393, 1509`): a 2,000-alias forward
  chain takes 2 s.
- **`vowc check` generates the C and discards it** (`vowc.vow:417`): skipping it takes
  35% off a check of the compiler.
- **Cache keys and lookup are quadratic in the number of functions**
  (`hash.vow:225-234`, `pipeline.vow:104-109`): 8,000 small functions take 10 s with a
  warm cache, against 0.5 s for `--no-verify`.
- **Done:**
  - locals are found through a map from each name to its place in the list
    (`Checker.local_at`; an entry is trusted only if that place still holds the name,
    so nothing is removed when a scope ends): 40,000 `let`s take 0.11 s, and 0.28 s
    when each binds a call;
  - aliases are resolved in one pass, in the order the rounds gave (`alias_turns`):
    the 2,000-alias chain takes 0.03 s;
  - `check` does not generate C: 0.20 s for the compiler, from 0.26 s;
  - cache keys and lookup were replaced in V5: 8,000 functions with contracts take
    0.55 s with a warm store, against 0.32 s for `--no-verify`.
- **Not done:** `ty_eq` still renders types. The depth limit (B1) bounds the case
  measured: a list literal nested 990 deep checks in 0.1 s.

### B6. Limits in generated C (reproduced by one reviewer)

- **An `if` with more than about 2,000 `elif` arms does not compile** (each arm nests
  one `else {` deeper; `codegen.vow:1283-1289`): "bracket nesting level exceeded
  maximum of 2048".
- **A function with 1,000 `try` statements takes 8.5 s to generate, and 2,000 take
  104 s** (`gen_match`/`gen_scoped`; the parser nests each `try` in a match).
- **A panic in an `elif` condition names the wrong line** (`codegen.vow:1277-1287`
  never resets the source position): an overflow on line 6 is reported at line 5
  (reproduced).
- **Done.** `gen_if` writes the arms of an if with `elif` as a sequence: each tests
  its condition and, if it holds, runs its body and jumps past the rest (`goto
  done_tN`). `gen_match` does the same for cases, which a C compiler also failed on
  (clang crashed at 5,000). The source position is set to each condition. `gen_try`
  recognises the match that `try` is sugar for and generates the rest of the block at
  the same depth, releasing the value tried once its payload is taken out: 200 tries
  build in 0.25 s (from 11 s) and 990 in 0.8 s. A function can still have at most
  1,000 (B1: each nests the statements after it for the checker and verifier). A C
  compiler is still slow on very long chains (17 s for 10,000 `elif` arms, in its
  optimiser). Tests: the "Large programs" block in `tests/run.sh`,
  `tests/panic/elif_line.vow`.

### B7. Diagnostics that mislead (reproduced by one reviewer)

- A struct field named `end` fails to parse, though the spec allows keywords as field
  names (`parser.vow:269`).
- A local with a module alias's name is reported at the function's position
  (`modules.vow:892-895`).
- A type from a module the file does not import appears under its internal tag
  ("this is util.Point").
- With a user-declared `Result`: "'try' ... needs Result, but this is Result".
- `f(1, 2)` reports "arguments to 'f' must be named" twice.
- **Done.** `parse_fields` takes an `end` followed by `:` for a field (nothing that
  closes a struct is followed by one). The resolver keeps the position of each local
  name (`Bound` in `modules.vow`), so the clash with a module is reported at the `let`,
  parameter, loop or case that binds it. `resolve_modules` records the file of every
  declaration with a qualified canonical name, and `show_names` uses it for a name the
  file's own display names do not cover: `Point (from lib/geo.vow)`, with a variant or
  type arguments kept next to the name (`Result<Int, String> (from
  <std>/prelude.vow)`). A match made from `try` on a Result without exactly the
  variants `Ok` and `Err(error: ...)` (and `value` in `Ok` when the value is bound) has
  its own message (`try_shape`); before, the errors named the variables the parser
  makes up (`unknown variable '20err'`). `match_args` reports unnamed arguments once
  for a call, with a note. Found on the way: `vowc fmt` took the `-` after a keyword
  field for a unary one (`s.end -1`). Tests: `tests/run/field_named_end.vow`,
  `tests/fail/{module_local_alias, module_unseen_type, try_own_result,
  result_prelude_named}.vow`, and the command-line block in `tests/run.sh`.
- **Not done.** `show_names` rewrites any `word.word` in a message that is a canonical
  name, so a message quoting source text in which a variable has the name of a file
  and a field the name of one of its declarations would be rewritten too. This was so
  before for the names a file imports.

### B8. Command line (reproduced by one reviewer)

- An unknown command is reported only after the file has been checked, verified and
  compiled (`vowc.vow:453`).
- Options before the file are rejected: `vowc check --no-verify f.vow` says "unknown
  option 'f.vow'" (`vowc.vow:325`).
- **Done.** `main` checks the command against `file_commands()` before anything else,
  takes the first argument that is not an option as the file wherever it is, and then
  checks each option against the commands that use it (`option_commands`): one the
  command ignores is an error, as are arguments after `--` for a command other than
  `run`, a second file, no file, and `-o` with nothing after it. All exit with
  status 2. Tests: the command-line block in `tests/run.sh`.

## V. The verifier: time and stability

Measurements on the compiler checking itself (585 functions, 17,500 lines):

| Measurement | Time |
|---|---|
| `vowc check vowc.vow --no-cache` | 34 s |
| The same with `--no-verify` | 0.14 s |
| Z3 alone on the same script | 32.8 s |
| Warm cache | 0.26 s |
| Z3 on the script without the functions that have nothing to prove | 1.2 s |
| The same, with sanity checks run without model-based instantiation | 0.82 s |
| The same, split across four Z3 processes | 0.24 s |
| One Z3 process per function, serial | 36 s |

The driver already uses one Z3 process with `push`/`pop` per function and per
obligation, so "incremental solving" in the usual sense is done.

### V1. 90% of verification time is three sanity checks on functions with nothing to prove (reproduced)

- **Where:** `compiler/verify.vow:3520-3523` (`script_for`), `compiler/pipeline.vow:112`.
- **What:** every function gets a `(check-sat)` on its definitions, to catch
  inconsistent definitions that would prove everything. 383 of the compiler's 585
  functions have no obligations at all, so there is nothing for that check to protect;
  for `std_text`, `std_json` and `runtime_header` (text built with `++`) it runs the
  full 10 seconds, answers `unknown`, and the answer is ignored.
- **Reproduce:** dump the script (`VOW_SMT_DUMP`), drop every function block with a
  single `check-sat`, and run Z3 on both: 32.8 s (582 `sat`, 3 `unknown`, 1101
  `unsat`) against 1.2 s (202 `sat`, 1101 `unsat`).
- **Fix:** emit nothing for a function with no obligations; run the sanity check with
  `smt.mbqi false` (`unknown` already passes, and a ground inconsistency still answers
  `unsat`). `make bootstrap` runs this check twice and `make test` several times more.
- **Done:** functions with no obligations are left out of the script (33.5 s to 1.9 s
  for `vowc check compiler/vowc.vow --no-cache`; the same 1,102 proofs). The
  `smt.mbqi false` part was measured and not taken: every sanity check then answers
  `unknown`, so the check would only catch what E-matching finds, for 0.4 s. With
  `VOW_SMT_V03=1` every function is still sent, as the frozen verifier does.

### V2. Run chunks of functions in parallel Z3 processes (measured by one reviewer)

- After V1, splitting the script into four contiguous chunks takes 0.82 s to 0.24 s.
  Eight chunks were no faster. One process per function is slower (36 s serial, 10 s
  with eight workers).
- Ask for the model in the same run (`(get-value ...)` after each `check-sat`,
  ignoring the error after `unsat`), instead of one more Z3 process per failed
  obligation (from reading).
- **Done.** `proof_scripts` divides the functions, in order, among up to `z3_jobs()`
  scripts of about the same amount of text (8, or `$VOW_JOBS`), and `run_z3_all`
  writes them to a temporary directory and has one `sh` start a Z3 for each, as many
  at once as the machine has processors (`getconf _NPROCESSORS_ONLN`). The compiler's
  cold check goes from 1.7 s to 0.6 s (Z3 alone from 1.6 s to 0.3 s with eight
  processes). Starting them costs about 25 ms, so a script gets at least 100,000 bytes
  (`script_share`) and a small program is proved by one process, with no shell. If the
  shell cannot be run, the scripts run one after the other. The dump (`VOW_SMT_DUMP`)
  and the answers are the same with one process and with several.
- **Done, another way.** The models: the scripts that ask for them (one per failed
  obligation, as before) are run together through `run_z3_all` when there are three
  or more. Asking in the main run would have put an error after every `unsat`.
- **Not done.** A program with little text but hard proofs (a few functions that each
  run to the work limit) is still proved by one process: how long a query will take
  cannot be told from its text.

### V3. There is no overall time limit (reproduced by one reviewer)

- **Where:** `compiler/verify.vow:3489-3491` (`run_z3`), `:3504`, `:3675`.
- **What:** the only limit is Z3's 10 seconds per query. A function with k hard
  obligations costs k x 10 s; Z3 sometimes overruns its own limit (one of twelve runs
  of a three-query script took 2 min 37 s); and if the solver hangs, `vowc` waits
  forever.
- **Fix:** a wall-clock limit in `vowc` that kills the child, and stop sending a
  function's obligations after its first timeout.
- **Done, in part:** Z3 is started with `-T:<seconds>`, after which it ends itself
  (120 s plus 30 s for each query, so only a solver that hangs meets it), and `vowc`
  reports "Z3 answered n of m queries and then stopped answering". A query's own
  clock is now a 30 s backstop behind the work limit of V4. With that limit, a
  function with k hopeless obligations costs k times 0.5 to 6 s, not k times 10 s:
  the fuzzer's seed 1000061, with eight of them, checks in 21 s, from 60 to 66 s.
- **Not done:** obligations after a function's first failure are still sent (the
  script is written before Z3 starts). The whole-run limit is long for a large
  program; a tighter one needs `vowc` to watch the solver while it runs.

### V4. The same query can get a different answer depending on what ran before it (reproduced by one reviewer)

- **What:** all functions share one solver and the limit is wall-clock time. Three
  sanity checks answer `unknown` in sequence and `sat` alone; one does the reverse.
  All 1,101 obligations agreed both ways, but an obligation near the limit could pass
  on a cold run and fail after a one-function edit, or the reverse.
- **Fix:** bound queries with Z3's `rlimit` (a deterministic resource count) in place
  of the timeout.
- **Done:** each query is limited to 2,000,000 resource units (`query_budget` in
  `compiler/verify.vow`), and each function is proved after `(reset)` with only the
  declarations it names, so its queries see nothing of what came before. Measured on
  the fuzzer's seed 1000061 with the functions in three orders: in one solver, 65 of
  102 queries used different amounts of work from order to order (one used 52,587
  units in one order and ran past 5,000,000 in the others); with a solver each, every
  obligation used the same amount in every order. Across 6,448 queries (tests,
  examples, standard library, compiler) the largest proof needs 123,223 units, and
  Z3 5.3.2 and 5.4.1 give the same answers and counts for all of the compiler's.
  The sanity check now runs with model-based instantiation off: it still answers
  `unsat` for contradictory definitions, and no longer spends the budget building a
  model (the compiler's cold check would otherwise go from 1.4 s to 3.2 s; it is
  unchanged). Verifier version `vow-verifier-15`. Tests: `tests/run.sh` compares a
  function's queries sent alone and with another function.

### V5. Editing any one-line function or any type re-verifies the whole program (reproduced by one reviewer)

- **Where:** `compiler/hash.vow:157-177` (`types_hash`).
- **What:** every function's key includes the text of all type declarations and all
  specification functions, used or not. Changing the body of `tested_z3_version`, or
  adding an unused struct, re-verifies all 585 functions; changing a multi-statement
  body re-verifies one. The cache is also per root file, so `vowlsp.vow` re-proves the
  compiler's functions even when `vowc.vow.vowcache` is warm.
- **Fix:** hash only the types and specification functions the function can reach
  (`total.vow`'s `proof_callees` computes most of that).
- **Done, another way:** the key is a hash of the verifier version and of exactly what
  the function's proof sends to Z3 (`query_key` in `compiler/pipeline.vow`), so it
  needs no list of what a proof can reach, and cannot miss an entry of one. Generating
  every function's queries on each run costs 0.04 s for the compiler; a warm check is
  0.19 s (0.18 s before), and no longer starts Z3. Editing `tested_z3_version` proves
  one function again (`main`, which inlines it), and adding a struct none. Tests:
  `tests/cache/field_refine`, `spec_refine` and `alias_refine` (a proof must not be
  reused), and the edits to `tests/reuse/main.vow` in `tests/run.sh` (exactly the
  functions an edit reaches are proved again).
- **Done too:** the per-file caches are gone. Proofs are kept in one store for every
  program, `~/.cache/vow` (or `$XDG_CACHE_HOME/vow`, or `$VOW_CACHE`): an empty file
  per proved key, so programs running at once need no lock. Checking `vowlsp.vow`
  after `vowc.vow` proves 26 functions, not 185. Keys are 128 bits. Nothing prunes the
  store; the directories of older verifier versions are dead and can be deleted.

### V6. A script's size is quadratic in the facts on a path (measured by one reviewer)

- **Where:** `compiler/verify.vow:3525-3540`: each obligation re-asserts every fact of
  its path. One function with 2,000 consecutive asserts gives a 57 MB script and 5.4 s
  in Z3.
- **Fix:** assert each new fact once, in nested `push` levels that follow the path.
  No gain at today's sizes; it removes the blow-up.
- **Done.** `fn_block` keeps the facts asserted in nested levels: for each obligation
  it pops the levels holding facts the obligation does not have and pushes one with
  those it lacks (and one for the part shared with the obligation before, when that
  was inside a larger level, so that the next one keeps it). 2,000 asserts in a row:
  57 MB and 8.4 s before, 220 KB and 0.17 s now. Measured on 6,531 queries (compiler,
  language server, examples, `tests/run`, `total`, `panic`, `vtest`): every answer is
  the same, the work in all is 11% less, and the largest proof needs 141,373 units
  (123,223 before; the limit is 2,000,000). Verifier version `vow-verifier-16`. The
  v0.3 layout (`VOW_SMT_V03`) and the scripts that ask for a model are as they were.
- **Not done.** A function of N steps that each assign a variable and assert about it
  still takes Z3 time that grows with the square of N (1,000 steps 3 s, 2,000 steps
  19 s): the script is linear, but every query has all the function's definitions.

### V7. A function that mentions a Float never gets a counterexample (reproduced by one reviewer)

- **Where:** `compiler/verify.vow:699` (`axioms: ... or v.floats`), `:3684`;
  `compiler/vowc.vow:402-407`.
- **What:** model-based instantiation is turned off for the whole function, so a false
  claim about its integers answers `unknown`. The message is "not proved: it may be
  false, or need facts ...", followed by a note that Z3 5.3.3 "did not decide some
  proofs; vowc is tested with Z3 5.3.2", which no version would decide. Remove the
  unused Float parameter and the same function prints `counterexample: m = -1, n = 0`.
- **Fix:** turn it off per obligation, for those whose facts or goal use the
  axiomatised symbols; do not count this case as undecided.
- **Done, another way.** The six Float axioms were sent for every function that
  mentions a Float; `float_axiom` now sends the axiom of an operation only when the
  function's queries use it, so a function that only has a Float parameter, or
  compares and adds Floats, has none and gets counterexamples. Turning model-based
  instantiation on for single obligations was tried and does not work while an axiom
  is asserted: with the `int_to_float` axiom alone Z3 runs to the work limit. Instead,
  in a function with axioms each answer is followed by `(get-info :reason-unknown)`:
  "incomplete quantifiers" is the undecidable case, reported as before plus the values
  Z3 had in mind as a "possible counterexample", and not counted as undecided (no note
  about the Z3 version); any other reason is Z3 giving up, and is now reported as that.

### V8. Counterexamples omit `old(x)` and can show states no run reaches (reproduced by one reviewer)

- **Where:** `compiler/verify.vow:594-621`.
- **What:** for `ensures a > (old(a) + 5)` the counterexample shows the final `a` and
  not `old(a)`. After a loop without invariants it can show `s = -1, len(xs) = 0`,
  which no run produces (the state is the loop's summary).
- **Fix:** show `old(p)` for inout parameters the goal names; say "possible state
  after the loop" when a shown variable was assigned in a loop.
- **Done.** `obligation` shows `old(p)` for every inout parameter whose value has
  changed (whether or not the goal names it), and records which of the variables it
  shows have been assigned in a loop met so far in the function (`Verifier.looped`);
  `failure_diag` adds a note naming them. The record is by name and in program order,
  not by path: a variable assigned in a loop on another branch is named too.

### V9. With `--json`, solver trouble is plain text on stderr (reproduced by one reviewer)

- **Where:** `compiler/vowc.vow:397-400`; `compiler/verify.vow:3650-3655`.
- **What:** with Z3 missing, `vowc check --json` prints nothing on stdout. A solver
  killed mid-run gets the hint "is Z3 installed?". A Z3 error is reported with the
  script's line number and no function name.
- **Fix:** a JSON diagnostic for driver errors, naming the function.
- **Done.** `discharge` reads the answers script by script and knows which function
  each is for, so an `(error ...)` or a run that stops short names the function it
  was in (`Discharged.fn_name`). A Z3 that is not there (status 127, no answers) is
  reported as that, and one ended by a signal no longer gets "is Z3 installed?". With
  `--json`, `vowc` prints the error as a diagnostic at the function, or at line 0.

## K. Checked builds (`--check-contracts`)

Checked builds are the oracle for the fuzzers, so a false alarm here looks like a
soundness bug, and slowness limits what can be fuzzed.

### K1. A contract that holds panics, because it is evaluated in 64 bits (reproduced)

- **Where:** `compiler/codegen.vow:913-929` (`gen_check`), `:1692-1697`.
- **What:** contracts are proved over mathematical integers and evaluated with
  trapping 64-bit arithmetic.
- **Reproduce:** `fn mid(lo, hi)` with `ensures (result * 2) <= (lo + hi)`, called with
  `lo = 9223372036854775800, hi = 9223372036854775806`: plain prints
  `9223372036854775803`; checked prints `vow: panic at s1.vow:6: integer overflow in
  '*'`, naming the `return` line.
- **Fix:** evaluate specification arithmetic in `__int128` (or saturate) while a
  contract is being checked; at the least, say "overflow while checking postcondition"
  at the contract's position, so the fuzzers can tell it apart.
- **Done:** a checked build evaluates the arithmetic written in a specification
  (`+`, `-`, `*`, `/`, `%` and unary `-` on `Int`, in contracts, refinements, loop
  invariants and `assert`) in `__int128` (`gen_wide` in `compiler/codegen.vow`, the
  `vow_w*` helpers in the runtime). Comparisons compare the wide values; where a value
  is needed (an argument, an index) it is narrowed, and must fit. When 128 bits are not
  enough, or a narrowed value does not fit, the panic is `cannot check <the
  specification> (...)` at the specification's position, which the fuzzers do not count
  as a finding. Tests: `tests/run/exact_specs.vow` (under `tests/rc.sh` with
  `--check-contracts`), `tests/panic/contract_beyond.vow`, `contract_narrow.vow`,
  `contract_exact_false.vow`.
- **Not done:** measures are still evaluated in 64 bits, and so is a function that a
  specification calls: `ensures result <= sq(n)` with `fn sq(n) do return n * n end`
  traps inside `sq`, though the verifier inlines it.

### K2. Every in-place update through an `inout` parameter copies (reproduced)

- **Where:** `compiler/codegen.vow:1058-1067` (`gen_entry_checks`).
- **What:** every `inout` parameter's entry value is captured whether or not a
  contract uses `old(...)`, so the parameter is never uniquely referenced.
- **Reproduce:** `fn count(m: inout HashMap<Int, Int>, k: Int)` that inserts, called n
  times: 40,000 calls take 0.006 s plain and 5 s checked; 80,000 take 21 s.
- **Fix:** capture only the parameters that `old(p)` names.
- **Done:** `gen_entry_checks` captures the entry value of an `inout` parameter only
  when `old(p)` appears in a postcondition or a loop invariant of the function
  (`olds_named` in `compiler/hash.vow`). 40,000 inserts in a checked build: 5.2 s to
  under 0.01 s.

### K3. Whole values are re-checked at every step (measured by one reviewer)

- **Where:** `codegen.vow:755-758`, `:1038-1048` (refinements); `:1108-1114`,
  `:1804-1831` (structural measures).
- **What:** after each `push` to a `List<Nat>` every element's refinement is checked
  again; a `decreases t` measure recomputes the height of the whole value at every
  call. 80,000 pushes: 0.025 s plain, 1.14 s checked. A recursive walk of a
  40,000-link chain: 0.008 s plain, 1.8 s checked.
- **Fix:** check only the new element when the list is moved; compare ranks lazily.
- **Done:**
  - Measures: when a part of a function's measure is a parameter (not inout), its rank
    is no longer computed on entry. A call compares the argument with the parameter by
    `vow_rank_cmp` (runtime): the same object has the same rank; an argument met on a
    short walk of the parameter (256 steps; every part of a value and every element
    of a list is looked at before the walk goes deeper) is smaller if it lies below an
    enum or a list, and the same if it was reached only through structs with one
    ranked field; otherwise both ranks are computed, the parameter's once per call of
    the function. A 40,000-link walk in a checked build: 7.5 s to under 0.01 s.
  - Refinements: after `set xs = push(list: xs, item: x)` only the new element's
    refinement is checked (`pushes_onto`). 80,000 pushes: 1.4 s to under 0.01 s.
  - Tests: `tests/run/lazy_ranks.vow`, `tests/panic/contract_struct_part.vow` and
    `contract_struct_fields.vow`; the earlier measure tests still panic as they did.
- **Not done:** a loop's structural measure and a measure part that is not a whole
  parameter are still computed each time; other updates of a refined list (`set_at`,
  `++`, an inout call) still check every element.

## P. Speed of the runtime and of generated code

### P1. String `++` always copies, so proved string builders are quadratic (reproduced by two reviewers)

- **Where:** `runtime/vow_rt.h:395-403` (`vstr_cat`); `codegen.vow:493` excludes
  strings from the move that lists get.
- **What:** 400,000 appends of two bytes take 3 s. The docs say to use `join`, but the
  verifier only tracks bytes built with `++`, so proved builders such as
  `examples/escape.vow` and `std/text.vow`'s `to_lower`/`to_upper` (0.9 s for 200 KB,
  134 s for 3.2 MB) have no fast path. A chain `a ++ b ++ ...` also allocates every
  intermediate and keeps them all until the statement ends.
- **Fix:** give strings a capacity, move the left operand for `set s = s ++ x`, and
  flatten chains into one concatenation.
- **Done:** `vstr_append_n` consumes its first string and extends it where it is when
  nothing else refers to it and its block has room; a string that outgrows its block a
  second time gets twice the room it needs (the room is kept in the header's unused
  mask word, so strings are no larger). A chain of `++` is flattened and joined in one
  call. `set s = s ++ x` moves `s`, and `set r = r with (f: r.f ++ x)` moves the
  field, as for lists. A benchmark of four builders (400,000 appends, a three-part
  chain, a struct field, `to_upper` of 800 KB) went from 12.2 s to 0.02 s. Test:
  `tests/run/string_append.vow`.

### P2. The documented "remove the last element" idiom copies the list (reproduced by one reviewer)

- **Where:** `runtime/vow_rt.h:709-721` (`vlist_slice`), `:638-660` (`vlist_push`).
- **What:** `slice(of: xs, from: 0, to: len(xs) - 1)` (the idiom in
  `docs/VOW_FOR_LLMS.md:683`) is O(n), so a stack is quadratic: 200,000 pops take
  3.75 s.
- **Fix (tried in a copy; all `tests/run` programs still pass with no leaks):** a
  slice from 0 shares the buffer, and `push` truncates in place when it is the only
  owner. 200,000 pops then take no measurable time.
- **Done.** With B6, `vlist_cut` shortens an unshared list where it is (the idiom
  written directly in a `set`). Now a slice from 0 of a list with other references (a
  parameter, in a helper) is a new list on the same buffer, and `vlist_push` releases
  the elements past the end of a list that has become the buffer's only one and
  appends there. The buffer is shared only when the piece is at least half of what it
  holds, so a short piece does not keep a long list's elements alive. A stack popped
  through a helper: 200,000 pushes and pops in 0.3 s, from 41 s. Test:
  `tests/run/slice_shared.vow`.

### P3. Updating one element of a list of structs allocates a struct (measured by one reviewer)

- **What:** `set ps = set_at(list: ps, at: i, value: ps[i] with (x: ...))` reads the
  element borrowed, so `with` copies it. 20,000,000 updates: 0.44 s, against 0.02 s
  for a `List<Int>`. `bench/nbody` is written as a struct of lists to avoid this.
- **Fix:** move the element out of the slot when the list is unshared.
- **Done.** `steal_candidates` recognises `set xs = set_at(list: xs, at: i, value:
  xs[i] with (...))` (the index the same variable or literal both times, nothing
  passed inout), and the `with` takes its base with `vlist_steal`, after the new
  fields are computed: the element is moved out of an unshared list (its slot left
  empty until `set_at` fills it) and duplicated otherwise. 20,000,000 updates: 0.06 s,
  from 0.38 s. Test: `tests/run/list_elem_update.vow`.

### P4. Every `match` and `for x in xs` dups and drops its subject (measured by one reviewer)

- **Where:** `compiler/codegen.vow:1304-1312`, `:840-845`, `:768-776`.
- **What:** the copy matters only for a `var` or `inout` parameter an arm could
  reassign. Removing it by hand from `bench/binarytrees` took depth 18 from 0.97 s to
  0.78 s.
- **Fix:** borrow when the subject is a parameter, `let`, binder or loop variable.
- **Done.** A function's generator knows the variables it assigns or passes inout
  (`Gen.changed`, from `changed_in`); a `match` or `for` whose subject is any other
  variable, or a field of one, borrows it (`stays`). A `var` that is never assigned
  counts as one that stays. `bench/binarytrees` at depth 18: 0.84 s, from 1.04 s.
  Test: `tests/run/borrowed_subject.vow`.

### P5. `value_or(opt: lookup(...), default: d)` allocates an Option per call (measured by one reviewer)

- **Where:** `compiler/codegen.vow:2048-2049, 2108-2110`; `vow_rt.h:987`.
- **What:** the counting idiom in the docs and in `bench/knucleotide`. Replacing it by
  hand with a direct read took 0.44 s to 0.36 s.
- **Fix:** recognise the pair in code generation (and `is_some(lookup(...))` as `has`).
- **Done.** `gen_looked_up` generates `value_or`, `is_some` and `is_none` of a
  `lookup` as `vmap_get_or`/`vhm_get_or` or `_has`, with the arguments evaluated in
  the order they are written. A counting loop of 25 million: 0.32 s, from 0.61 s;
  `bench/knucleotide`: 0.16 s, from 0.20 s. Test: `tests/run/lookup_direct.vow`.

### P6. Variants without fields allocate, and the object header is 16 bytes (measured by one reviewer)

- **Where:** `runtime/vow_rt.h:519-523` (`vow_variant0`); `None` is already a shared
  constant (`:525-530`).
- **What:** `Tree.Leaf` written naturally costs 35% more time and 65% more memory in
  binarytrees than the hand-shared leaf the benchmark uses.
- **Fix:** one static object per variant without fields. Then an 8-byte header with a
  per-type descriptor in place of the per-object mask: a three-word node goes from 48
  to 32 bytes, and the 63-field limit (A3) goes away.
- **Done, the first part.** `vow_variant0` keeps one object, never freed, for each
  tag below 256 (a variant without fields is only its tag, so the object is shared by
  enums too). A tree benchmark with `Tree.Leaf` written where a leaf is wanted: 26 MB
  and 1.54 s, from 43 MB and 1.67 s. Test: `tests/run/variant_shared.vow`.
- **Not done.** The 8-byte header. It changes the layout of every object the runtime
  makes and every place that reads a header (`with`, release, equality, measures,
  strings' capacity), and the generated C that makes objects. Its gain is memory for
  objects with an even number of payload words; it is a task of its own (P6b).

### P7. `float_to_str` makes up to 17 print-and-parse round trips (measured by one reviewer)

- **Where:** `runtime/vow_rt.h:447-451`. 3.2 microseconds a call, against 0.05 for
  `int_to_str`.
- **Fix:** try 15, 16, then 17 digits (1.2 s per million, identical output except for
  subnormals, which keep the loop), or a shortest-digits algorithm.
- **Done.** As proposed: 15 digits without their trailing zeros if they read back,
  else 16, else 17; subnormal numbers keep the loop. A million calls: 1.4 s, from
  3.8 s. Compared with the loop on 12 million doubles (random bits, short decimals,
  subnormals, large integers, neighbours of short decimals): the same text for all.
  Test: `tests/run/float_shortest.vow`.

## L. The language server

No crash or hang in 30,000 fuzzed edits, and malformed framing is survived.

### L1. Checking one file clears the diagnostics of every other open file (reproduced by one reviewer)

- **Where:** `tools/lsp/vowlsp.vow:271-290` (`s.published` is one global list).
- **What:** open `a.vow` with an error, then open an unrelated `b.vow`: `a.vow`'s
  errors are cleared while it is still broken. Closing an unsaved broken buffer leaves
  its diagnostics published.
- **Fix:** track published files per root document.

### L2. A function named `test_...` is a false error, and it hides every proof failure (reproduced)

- **Where:** `tools/lsp/vowlsp.vow:254, 405` call `check_source(test_mode: true)` for
  every file; `compiler/pipeline.vow:55-57`.
- **What:** `fn test_bit(n: Int, bit: Int) -> Bool` is reported as "test function
  'test_bit' must take no parameters and return nothing", and since there is then a
  diagnostic, verification never runs. Opening `compiler/vowc.vow` shows it
  (`fn test_names`).
- **Fix:** keep "no main needed", drop the test-shape rule unless the file is a test.

### L3. Positions are bytes, not UTF-16 code units (reproduced by one reviewer)

- **Where:** `tools/lsp/vowlsp.vow:171-177, 311-314, 576-577`.
- **What:** after non-ASCII text on a line, diagnostics and hovers are at the wrong
  column. A "cannot open" on the root file is published at line -1, character -1.
- **Fix:** convert with the line's text (or negotiate `positionEncoding: utf-8`), and
  clamp to 0.

### L4. Proof failures vanish on the first keystroke after a save (reproduced by one reviewer)

- **Where:** `tools/lsp/vowlsp.vow:146-161, 256`.
- **Fix:** keep the last proof diagnostics per file and republish them until the next
  proof run.

### L5. Hover and go-to-definition look up global names before local ones (reproduced by one reviewer)

- **Where:** `tools/lsp/vowlsp.vow:385-397, 413, 514, 531-545`.
- **What:** hovering a parameter named `text` in a file that imports `std/json` shows
  `fn json.text(...)`; a definition request on a pattern binder jumps to a `let` of
  the same name in another function.
- **Fix:** resolve the enclosing function's locals first.

### L6. A proof blocks the server, and stale edits are all processed (measured by one reviewer)

- **Where:** `tools/lsp/vowlsp.vow:24-39, 250-262, 399-405`.
- **What:** a hover sent after opening `vowlsp.vow` with a cold cache is answered
  after 23 s; ten queued edits are each fully checked; `$/cancelRequest` is ignored.
  V1 removes most of the 23 s.
- **Fix:** keep only the latest edit per document; prove in a child process that a
  newer edit can abandon.

## S. The standard library

### S1. `std/json` mishandles surrogates and accepts some invalid JSON (reproduced by one reviewer)

- **Where:** `std/json.vow:408-414, 319-327, 387-389, 321, 358`.
- **What:** `"\ud83dA"` drops the `A`; a lone surrogate becomes invalid UTF-8; `01`
  and a raw tab inside a string are accepted; `-9223372036854775808` becomes a Float;
  `1e400` prints back as `null`; error offsets are one past the offending byte.
- **Fix:** U+FFFD for a lone surrogate, and process what follows it normally; reject
  leading zeros and bytes below 0x20; special-case the minimum integer.
- **Done.** As proposed. A high surrogate followed by a `\u` escape that does not
  complete it is U+FFFD, and that escape is read as any other (it may begin a pair).
  A number too large for a Float is an error, not an infinity. The checks for a
  separator and for an escape come before the reader moves on, so the offset is the
  wrong byte's. Test: `tests/run/json_strict.vow`.

### S2. `json.parse` overflows the stack on deep nesting, which also kills the language server (reproduced by one reviewer)

- **Where:** `std/json.vow:225-303`. 100,000 nested brackets segfault; a framed body
  of 200,000 `[` kills `vow-lsp`.
- **Fix:** a depth counter in the reader, and `Err` past a limit.
- **Done.** `read_value` is given the depth and refuses an array or object inside
  1000 others (`json.max_depth()`). 100,000 brackets are an error at byte 1000.

### S3. `std/text` contracts are weaker than the README says (from reading)

- `contains` promises nothing when it returns false; `compare` only bounds its
  result; `replace` and `repeat` have no postcondition.
- **Fix:** `not result implies forall ...` on `contains`; `(result == 0) == (a == b)`
  on `compare`; lengths on `repeat`.
- **Done.** `contains` and `repeat` as proposed (`repeat` is built with `++`, which
  the verifier follows). `compare`: `(a == b) implies (result == 0)`, and `(result ==
  0)` implies the same length and the same byte at each index. `replace`: the same
  length when `old` and `new` have the same length, and the same bytes when `old`
  does not occur. Test: `tests/run/text_contracts.vow`.
- **Not done.** `(result == 0) implies (a == b)`: the verifier has no way to conclude
  that two strings are equal from their bytes (two empty strings are not provably
  equal), so no contract can promise `==` of a string that was built. A first
  version of `replace` that carried "nothing to replace so far" as a loop invariant
  needed 625,008 units of work for one proof (a third of the limit); it now looks for
  the first occurrence before the loop, and its largest proof needs 12,329.

## G. Packages and running programs

### G1. `vowc add <name> <url> main` can pin another branch (reproduced)

- **Where:** `compiler/packages.vow:316-338` (`resolve_ref`).
- **What:** `git ls-remote url main` matches every ref whose last component is `main`,
  and the first line is taken; `refs/heads/feature/main` sorts first.
- **Reproduce:** a repository with branches `main` and `feature/main`:
  `vowc add lib <repo> main` pins the commit of `feature/main`.
- **Fix:** query `refs/heads/<ref>`, `refs/tags/<ref>` and its peeled form exactly,
  and report an ambiguous name.
- **Done:** as above; `HEAD` and full names (`refs/...`) are also matched exactly. The
  fix found a second fault: an annotated tag was pinned to the hash of the tag, not of
  its commit, so the fetch after it failed ("did not check out commit"). Tests: the
  package block of `tests/run.sh`.

### G2. Package hardening (reproduced by one reviewer; each is minor)

- `vowc add` with a commit that does not exist writes the bad pin to `vow.pkg` before
  failing, replacing a good one (`packages.vow:342-381`).
- A relative URL in `vow.pkg` resolves against the working directory, not the file.
- URLs reach `git` without `--` (`packages.vow:263, 320`), so a URL starting with `-`
  is read as an option. No way to run a command was found.
- `import "@lib/../../outside.vow"` reads a file outside the package
  (`packages.vow:237`).
- **Done**, all four. `vowc add` fetches before it writes. A relative path is relative
  to its `vow.pkg` (and refused in a fetched package's `vow.pkg`, whose directory is
  the store). URLs follow `--`, and one starting with `-` is refused: contrary to the
  note above, `vowc add x "--upload-pack=touch <file>"` did run the command. An
  `@name` import with a `..` part is an error, and so is a relative import in a
  package's file that ends outside the package (the same escape from the other side,
  not in the review). Tests: the package block of `tests/run.sh`.

### G3. `vowc run` gives the program empty stdin and holds its output until it exits (reproduced)

- **Where:** `compiler/vowc.vow:443`: `run(argv: ..., input: "")`, then `print(o.out)`.
- **Reproduce:** `echo hello | vowc run echo.vow` prints `no input`; the built binary
  prints `got hello`. Stderr arrives before stdout, and an interactive or long-running
  program shows nothing until it ends.
- **Why it matters:** the 1.0 benchmark judges command-line programs by their input
  and output.
- **Fix:** run the program with inherited stdin, stdout and stderr.
- **Done:** a new built-in, `run_attached(argv) -> Int` (`uses proc`), runs a command
  with the program's own standard input, output and error and gives its exit status.
  `vowc run` uses it. It is the one built-in the compiler uses that is newer than
  v0.3, so the call is alone in `compiler/attached.vow`, and `tests/differential.sh`
  gives both verifiers `tests/v03/attached.vow` in its place. Tests:
  `tests/run/run_attached.vow`, and `tests/run.sh` pipes input through `vowc run`.

## T. Tests, CI and releases

### T1. `tests/rc.sh` does not count a failed build as a failure (reproduced)

- **Where:** `tests/rc.sh:25-28`: `build` prints "FAIL build" and returns 1, and no
  caller increments `fail`. A regression that stops checked or debug builds from
  compiling leaves `make test-rc` green.
- **Done:** `build` increments `fail`. With a flag that breaks every build
  (`RC_FLAGS=--bogus`) the script now reports "0 passed, 57 failed".

### T2. Three more harnesses can pass without checking anything (reproduced by one reviewer; confirmed by reading)

- **`tests/fingerprint.sh:21`:** every program's "source hash" includes all of
  `std/*.vow`, `tests/modules/*.vow` and `examples/*/*.vow`, so one edit to any of
  them disables the check for every program until `make fingerprint`.
- **`tests/differential.sh:17-19`:** the two dump files are not removed between
  programs, so if both compilers fail to write one, the previous pair is compared.
- **`tests/genfuzz.py:1584`:** a run in which nothing verified (no Z3, a broken
  compiler) exits 0.
- **`tests/run.sh:106-107`:** the formatting check counts a file `fmt` cannot parse,
  or a crash, as formatted.
- **Both fuzzers** class a verified program killed by a signal as "rejected" or
  "other", which is how A2 to A4 went unnoticed.
- **No test script has a timeout**, so a hang costs CI its six-hour default.
- **Done:** the fingerprint hashes each program with only the modules it imports;
  the differential test removes both dumps first, fails when one is missing, and fails
  when nothing was compared; the generator fuzzer exits 1 when nothing verified, and
  counts a verified program that fails without a panic as a finding; the mutation
  fuzzer builds and runs in two steps and counts a death by signal as a finding; the
  formatting check fails on a file `fmt` cannot read (outside `fail/`) or crashes on.
  Timeouts are per CI job (45 minutes), not per test.

### T3. The Release workflow publishes binaries without running the tests (from reading)

- **Where:** `.github/workflows/release.yml:6-8, 62-67`; `ci.yml`; `site.yml`.
- **What:** the workflow runs when a release is published, does not depend on CI, and
  smoke-tests three examples. Assets are uploaded after the release is already the
  latest, so `install.sh` gets a 404 for a platform until its job finishes (or for
  good, if it fails). The static Linux builds, linux-arm64, the x86-64 half of the
  macOS binary and `vow-lsp` are not exercised before packaging. `ci.yml` has no
  `permissions` and no timeouts; actions float on major tags; the site deploys
  `install.sh` from main whether or not CI passed.
- **Fix:** run `make test` before upload (or require CI for the commit); upload to a
  draft and publish last; `permissions: contents: read` and timeouts; pin actions.
- **Done:** the Release workflow runs `tests/run.sh` and the language server's test
  with the binaries it is about to package, and the x86-64 half of the macOS ones
  under Rosetta; a last job publishes the release if it was created as a draft (the
  workflow's header gives the steps). The macOS steps were rehearsed locally; the
  workflow itself first runs at the next release. CI has `contents: read`; every job
  has a timeout; actions are pinned to commits; the Site workflow runs after CI
  succeeds on main.

### T4. `install.sh` replaces a working install before trying the new binary (reproduced by one reviewer)

- **Where:** `install.sh:45-54`: `cp` over the old `vowc`, then
  `"$dir/vowc" version | head -1`, whose failure the pipe hides: "vow: installed  to
  ...", exit 0, with a binary that cannot run.
- **Fix:** run the new binary from the temp directory first; copy to a temp name and
  `mv` into place.
- **Done.** As proposed, for `vowc` and `vow-lsp`. `VOW_RELEASE_BASE` says where
  releases are fetched from, so that `tests/run.sh` can install releases it makes: one
  that runs, two whose `vowc` does not, and one with a wrong checksum.
- **Fine:** the checksum is verified, and a truncated download installs nothing.

### T5. The Makefile can leave an empty generated file, and `make test` does not see edits to `std/` or the runtime (reproduced by one reviewer)

- **Where:** `Makefile:66-76` (`... > $@` with no `.DELETE_ON_ERROR`), `:61`.
- **What:** if a generation step fails (Z3 missing), `compiler/runtime_h.vow` is left
  at 0 bytes and newer than its sources. `vowc` always uses its embedded copies of
  `std/*.vow` and the runtime, so after editing them `make && make test` runs against
  the old copies until `make bootstrap`.
- **Fix:** `.DELETE_ON_ERROR:` (or write to a temp file and `mv`), also for the seed;
  make `test` check that the embedded copies are current.
- **Done.** `.DELETE_ON_ERROR:`, and `make test` begins with `test-current`: each
  embedded file must be its source embedded, and `seed/vowc.c` what the compiler's
  source gives; otherwise it stops and asks for `make bootstrap`. (The seed is copied
  into place only after the fixpoint check, so it needed nothing.)

## Open tasks, reviewed

The nine tasks open in `ROADMAP.md` before this review:

- **Verifier performance (incremental solving):** the driver is already incremental.
  The time is in V1, then V2, V5 and V3. Replaced by those tasks.
- **Agent benchmark for 1.0:** do section A, G3 and P1 first. An agent that writes
  `set xs = xs ++ xs`, pipes input to `vowc run`, or builds a string in a loop would
  measure these bugs, not the language.
- **Publish a first package:** do G1 and the first item of G2 first.
- **Stability promise:** it freezes the answers to B2 (which number forms exist), A3
  (how many fields a type may have), A5 (whether stack overflow is a documented
  failure) and the first item of B7 (`end` as a field name). Decide them before the
  freeze.
- **More standard modules, a C FFI, new effects:** do P1 and S3 first, so that new
  modules do not copy `std/text`'s quadratic builders and thin contracts.
- **M9b Windows** and **Windows `run()`:** these are one task listed twice. The port
  rewrites `vrun`, which is where A6 is.
- **M11 back ends:** nothing found. The 2,000-arm limit in B6 is clang's, so a bundled
  `zig cc` has it too.
- **Lexicographic `Ord`:** nothing in this review needs it. Keep waiting.
