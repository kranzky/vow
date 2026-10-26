# Writing vowsite in Vow

The documentation and examples were enough to write this program without reading
the compiler's generated C. Verification was smoother than writing a correct
Markdown parser: the first version verified, but a later test exposed incorrect
inline rendering. This report distinguishes those two kinds of correctness.

## Numbers

- 545 lines of Vow, 23 functions. Only `main` declares effects (`fs, console`).
- 11 `while` loops, all with proved `decreases` measures; both recursive functions
  also have termination measures. No `check`, no `proc`, no `--no-verify`.
- Seven explicit check/build invocations for vowsite, counting the final clean
  build and excluding compiler-suite invocations and up-to-date Make targets:

  | Attempt | Command / reason | Result |
  |---|---|---|
  | 1 | `vowc check` on the first implementation | `ok` |
  | 2 | `make build/vowsite` | Built |
  | 3 | `vowc check --no-cache` | `ok` |
  | 4 | `vowc check` after fixing nested inline parsing | `ok` |
  | 5 | Rebuild through `make test-site` | Built |
  | 6 | Final source, `vowc check --no-cache` | `ok` |
  | 7 | Rebuild through `make clean && make && make site && make test` | Built |

- Compiler diagnostics: **0 parse errors, 0 type/effect errors, 0 verification
  errors**. There were no compiler or linker failures. These are observed counts,
  not an estimate of how often I reconsidered code before running the compiler.
- One failing Markdown fixture during development, fixed before delivery.
- Final tests: **13 Markdown fixtures, 13 integration checks, 29 existing tests**.
  Six site pages generated, including the four required pages. Roadmap and license
  copies keep the README's local links working.

## Friction log

Times are approximate, reconstructed from the working notes below.

1. **The standard library is very small (roughly 3 minutes of design).** There is
   no string split, substring search, prefix test, trim, or directory creation.
   I expected to need parser helpers, but a sizeable part of the program is basic
   text plumbing. There was no diagnostic: I checked the documented built-ins
   first. I wrote bounded pure helpers, retained original line endings in a
   `Line` struct, and made the shell/Makefile create output directories.

2. **The subset did not directly fit the source documents (about 3 minutes).**
   Browser inspection showed list continuation text outside its list item, and
   the README's indented fence was rendered as paragraph text. I had expected
   copying the documents and fixing links to be enough. The task's flat lists
   and column-zero fences do not cover that formatting. I reflowed list items,
   unindented the copied fence, and removed blockquote markers in `site/` only.
   No compiler diagnostic; original documents remain unchanged.

3. **The first delimiter search was incorrect (about 2 minutes).** A newly added
   `nested-inline` fixture produced this diff:

   ```diff
   -<p><em>Emphasis with <strong>strong</strong> inside</em>.
   +<p><em>Emphasis with </em><em>strong</em><em> inside</em>.
   ```

   `make` reported `make: *** [test-site] Error 1`. A double star inside inline
   code also closed an outer strong span prematurely. I expected nested markup
   and code spans to retain their boundaries. The fix was `emphasis_end`, which
   skips code spans and searches nested delimiters with a proved shrinking
   suffix. The expanded fixture now passes. This was my parser bug; verification
   had correctly proved bounds and termination, not Markdown semantics.

4. **Test summary count (less than a minute).** The initial shell message said
   `14 integration checks passed`, although there were 13. I counted the six
   successful rendering/manifest cases, six failure cases, and empty-manifest
   case, then corrected the message. All checks themselves had passed.

5. **Browser tooling (about a minute).** Browser discovery returned
   `No browser is available`. Native Chrome was available through computer use,
   so I used that to inspect the generated files. I checked all four required
   pages, including the corrected lists, code examples, and specification table.

6. **Optional performance measurement (less than a minute).** macOS `time -l`
   reported `time: sysctl kern.clockrate: Operation not permitted`. Site
   generation itself succeeded. I did not pursue an unnecessary permission
   change and make no reliable timing or peak-memory claim here.

## Documentation gaps

I found no statement in `VOW_FOR_LLMS.md` that I could demonstrate was wrong.
The most useful instructions were the explanations of loop havoc, callee
contracts, expression predicates, and non-mixed binary operators.

Concrete additions that would have helped:

- A small text-processing recipe: bounded `find`, prefix testing, splitting LF
  and CRLF while retaining EOF information, and a failure index contract. The
  guide's examples cover much of the reasoning, but not this everyday combination.
- A complete terminating recursive parser example with both a shrinking substring
  and a scanning loop. The guide explains recursive measures briefly; the working
  examples I used primarily demonstrate loop proofs.
- A CLI recipe for the `fs, console` effect combination: output directories must
  be created outside the program; `exit` needs `proc`, whereas `panic` can end an
  error path without it. The signatures are documented, but the practical shape
  of a restricted batch program is easy to miss.
- Guidance on proving properties of a string assembled with `join`. The guide
  recommends collecting parts for performance, while its proved escaper uses
  repeated concatenation. A demonstrated way to combine those two patterns would
  be useful. I did not test whether a stronger proof for `join` is possible.

The guide's `while more` example intentionally has no measure. It is valid Vow,
but not a pattern one can copy unchanged into this task, which requires total
termination proofs. This is a distinction to highlight, not a compiler defect.

## Verification experience

**Easy:** the escaper's quantified `tag_free(result)` postcondition and loop
invariant worked using the technique from `examples/escape.vow`. Range loops
supplied indexing bounds; string bytes were accepted by `char`. `find` and
`link_end` return contracts made subsequent slices straightforward.

**Most reasoning before compilation:** nested block loops. The renderer consumes
at least one line before entering each inner loop, saves that position as
`start`, and preserves `start <= i <= len(src)`. Thus the outer loop still knows
it progressed, including when an inner list/table loop breaks. The two recursive
inline helpers use `len(s)` and `len(s) - start` as their measures. Each recursive
call sees a strictly shorter input or remaining suffix.

No required proof was abandoned, weakened into `check`, or bypassed. There were
no failed proof obligations, solver timeouts, or counterexamples, so I cannot
claim counterexamples helped in this session. The deliberate bounds/progress
structure probably accounts for the lack of solver friction.

The proved property of `escape` is specifically absence of literal angle
brackets. Fixtures additionally check escaping of ampersands and double quotes.
I did not formally prove the whole renderer equivalent to a Markdown grammar,
that URLs use safe schemes, or that every generated HTML fragment is balanced.
Those properties received runtime tests and an independent HTML audit instead.

## Language assessment

Named arguments helped with functions carrying several positions and strings.
Pure-by-default functions made the I/O boundary obvious. Explicit contracts on
search helpers were useful design constraints, not just proof annotations.
Labelled ends made the deeply nested parser unambiguous.

The cost was verbosity. No precedence demands many parentheses in simple bounds
tests. No shadowing requires distinct names throughout nested control flow. A
Boolean prefix helper does not automatically expose its internal bounds facts,
so I used explicit guards where a later slice needs them. This is consistent with
signature-only reasoning, but a less repetitive predicate-contract idiom would
help. I missed ordinary string utilities, a string builder with verification
support, filesystem directory creation, and structured I/O error information.

Most output uses lists of fragments and `join`, following the guide. The proved
escaper retains the example's repeated concatenation. Consequently, a single
very long escaped span can have quadratic copying/allocation cost. The runtime
does not reclaim memory, and recursive inline parsing also consumes stack.
This is a batch documentation generator, not a bounded-memory streaming parser.
Termination proofs alone do not establish resource limits.

The implementation deliberately remains a Markdown subset: flat lists, no raw
HTML, no backslash-escape or complete CommonMark delimiter/flanking algorithm,
and no table alignment syntax. It preserves UTF-8 bytes, code line endings, and
missing final newlines. Invalid/missing input and failed output writes are tested;
generation is not transactional if an I/O error occurs after an earlier page was
written. The site template is trusted markup; Markdown and titles are escaped.

## Compiler bugs and reproductions

**No compiler or runtime bug was found.** There is no minimal compiler-bug
reproduction to provide. The nested-inline fixture above is a minimal regression
for my own parser mistake. I did not read `seed/vowc.c`, and did not modify the
compiler, runtime, original README, or `docs/`.

## Final validation

- Clean compiler build from the C seed, followed by `make site` and `make test`:
  all succeeded.
- Final source verified with `build/vowc check tools/vowsite/vowsite.vow
  --no-cache`: `ok`.
- Exact fixture output and all I/O/template integration cases passed.
- Independent HTML parsing found balanced tags, one H1 and a title on each of
  the six pages, and no missing local link targets or unexpected script elements.
- Chrome visual inspection completed for `index`, `spec`, `llms`, and `humans`.
- Work is on `m12-vowsite`. This sandbox has no configured Git remote or CI/PR
  workflow, so validation is local.

## Notes recorded during implementation

- Read the complete LLM guide and specification, then the escape, cursor and
  word-frequency examples. Did not read the generated compiler C or change the
  compiler, runtime or source documentation.
- Baseline: `make && make test` succeeded: 29 passed, 0 failed.
- Working branch: `m12-vowsite`. There is no configured Git remote in this sandbox.
- Design: only `main` performs I/O; shell creates directories. Preserve code-block
  line endings, scan templates once, and keep index/progress contracts on helpers.
- Compile attempt 1: `build/vowc check tools/vowsite/vowsite.vow` printed `ok`
  immediately. Parse/type/verification errors so far: 0/0/0. The example escaper
  proof worked unchanged in structure. Conservative bounds and progress invariants
  avoided the guide's documented loop pitfalls.
- Compile attempt 2: `make build/vowsite` built successfully. All 12 initial
  Markdown fixtures and 13 integration checks passed. The first test-summary
  message mistakenly said 14 integration checks; corrected the count to 13.
- Compile attempt 3: uncached `vowc check ... --no-cache` printed `ok`.
  `make site` generated six pages, and `make test` passed both suites.
- Browser inspection exposed a content/subset mismatch: the source docs have
  soft-wrapped list items and an indented fence, but the specified subset has flat,
  single-line list items and column-zero fences. The browser showed list
  continuation text as separate paragraphs. Reflowed only the copied site list
  items, unindented that copied fence, and removed blockquote markers in the site
  copy. Original docs remain untouched. About 3 minutes; no compiler diagnostic.
- Browser connector discovery said `No browser is available`; native Chrome was
  available through computer use, so used that for the visual review.
- A new nested-inline fixture failed: `*Emphasis with **strong** inside*` became
  three separate emphasis spans, and a delimiter inside inline code closed the
  enclosing strong span. This was my parser bug, not a compiler bug. Replaced raw
  delimiter searches with a search that skips code and nested emphasis, with its
  own recursion and loop termination measures. About 2 minutes.
