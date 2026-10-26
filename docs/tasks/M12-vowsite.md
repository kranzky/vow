# Task M12: `vowsite`, a static site generator written in Vow

You are building a real program in Vow, a new language designed for language models
to write. This task has two equally important outputs: the program, and an honest
report of what it was like to write it.

## Before you start

1. Read [`docs/VOW_FOR_LLMS.md`](../VOW_FOR_LLMS.md) completely. Use
   [`docs/SPEC.md`](../SPEC.md) as the reference, and `examples/` for working code.
2. Build the compiler and run the tests: `make && make test`. You need a C compiler
   and Z3 (`brew install z3` or `apt install z3`).
3. **Do not read `compiler/` or `bootstrap/`** unless you are blocked. They contain a
   lot of Vow, but this task measures whether the documentation is enough. If you do
   read them, say when and why in your report.
4. Do not change the compiler, the runtime or the docs. If you find a compiler bug,
   write it down, work around it, and keep going.

## What to build

`tools/vowsite/vowsite.vow` reads Markdown pages and writes HTML pages.

**Input**
- `site/pages.txt`: one page name per line (for example `index`). Vow has no directory
  listing, so this manifest says which pages exist.
- `site/<name>.md`: the page source. The page title is its first `# ` heading.
- `site/template.html`: an HTML template containing `{{title}}` and `{{content}}`.

**Output**
- `public/<name>.html` for every page in the manifest: the template with `{{title}}`
  replaced by the escaped title and `{{content}}` by the rendered page.

**The Markdown subset** (anything else is rendered as a plain paragraph):
- headings: `# `, `## `, `### ` at the start of a line;
- paragraphs: consecutive non-blank lines, separated by blank lines;
- unordered lists: lines starting with `- `; ordered lists: lines starting with
  `1. ` (any number followed by `. `);
- fenced code blocks between lines that are exactly three backticks (an optional
  language name after the opening fence may be ignored); content is escaped and
  otherwise untouched;
- pipe tables: a header row, a separator row of `|---|---|` cells, then body rows;
- inline: `` `code` ``, `**strong**`, `*emphasis*`, and `[text](url)` links.

All text taken from the input must be HTML-escaped (`&`, `<`, `>`, `"`), including link
URLs and code. Only tags that `vowsite` generates itself may appear in the output.

**The site.** Create `site/` with pages for the README (`index`), `docs/SPEC.md`
(`spec`), `docs/VOW_FOR_LLMS.md` (`llms`) and `docs/VOW_FOR_HUMANS.md` (`humans`).
You may copy those files into `site/` and adjust links to point at `.html` pages. Add
a simple, readable template with navigation between the pages.

## Requirements on the Vow code

- `main` may use `fs` and `console`. **Every other function must be pure** (no
  `uses` clause), and nothing may use `proc`.
- The escaping function must carry a proof that its result contains no `<` or `>`
  (see `examples/escape.vow` for the technique). Use it for every piece of input text.
- Every `while` loop must have a `decreases` measure, so the parser is proved to
  terminate. `for` loops terminate on their own.
- Avoid `check`. If you use it, explain in a comment why the fact can't be proved.
- The program must verify: `build/vowc check tools/vowsite/vowsite.vow` prints `ok`.
  `--no-verify` is not acceptable for the final result.

## Tests and build

- Add `tools/vowsite/tests/`: pairs of `<case>.md` and expected `<case>.html`
  fragments covering every construct above, plus escaping edge cases (`<script>`,
  `&amp;`, quotes in link URLs, unterminated `**`, an unclosed code fence).
- Add a way to run those tests (a small Vow test driver, or a shell script).
- Add a `site` target to the `Makefile` that builds `vowsite` with `build/vowc` and
  generates `public/`. Add `public/` to `.gitignore`.

## The report: `tools/vowsite/REPORT.md`

Keep notes as you go. This is the most valuable part of the task. Include:

1. **Numbers:** roughly how many compile attempts it took, and how many errors of each
   kind (parse, type, verification).
2. **Friction log:** every time you were stuck or surprised, with the error message,
   what you expected, what fixed it, and how long it took. Be specific and quote
   messages.
3. **Documentation gaps:** what `VOW_FOR_LLMS.md` should have told you but didn't, and
   anything in it that was wrong.
4. **Verification:** which proofs were easy, which were hard, which you gave up on, and
   why. Did counterexamples help?
5. **Language:** features you missed, rules that helped, rules that only got in the way.
   Be candid. The point is to improve Vow, not to praise it.
6. **Compiler bugs** you found, with minimal reproductions.

## Done means

- `make site` produces `public/index.html`, `public/spec.html`, `public/llms.html` and
  `public/humans.html`, and they look right in a browser.
- The `vowsite` tests pass.
- `make test` still passes.
- `tools/vowsite/REPORT.md` is complete.
- Everything is committed on a branch named `m12-vowsite`.
