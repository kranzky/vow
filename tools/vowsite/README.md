# vowsite

From the repository root:

```sh
make site
make test-site
build/vowc check tools/vowsite/vowsite.vow
```

Open `public/index.html` in a browser. The executable is `build/vowsite`; it
reads `site/pages.txt` relative to its working directory and writes
`public/<name>.html` for each page. The destination directory must already exist
(`make site` creates it). `make test` includes the vowsite tests. The site's
workflow (`.github/workflows/site.yml`) publishes `public/` to vow.kranzky.com.

Each manifest line is `name`, `name source` or `name source template`; the
defaults are `site/<name>.md` and `site/template.html`. A line `@base <url>` sets
where relative links to files that are not pages go. The site is built from the
repository's own documents (README, specification, guides, comparison, roadmap,
this report and the license), so it cannot drift from them. Relative links between
those documents, such as `docs/SPEC.md` or `../ROADMAP.md#v1-scope`, become links to
their pages; other relative links resolve against `@base`.

Codex wrote vowsite as task M12 (see `docs/tasks/M12-vowsite.md`). It was later
extended to render the documents directly: list items may continue on indented
lines and contain paragraphs and code fences, a blank line between items makes a
loose list, `##` and `###` headings get `id` anchors, and links are rewritten as
above.

Manifest names use letters, digits, hyphens and underscores. Blank manifest lines
are ignored; duplicate names, invalid names, missing inputs and failed writes
produce a diagnostic and a nonzero exit status. A page without a level-one heading
uses its name as the title. Headings inside fenced code do not supply the title.

The parser implements the Markdown subset in the task, not full CommonMark.
Lists are not nested. Tables need matching column counts and separator cells containing
at least three hyphens; outer pipes are optional, and pipes inside inline code
are retained. Unsupported block syntax stays paragraph text. Unclosed code fences
extend to EOF, preserving original code whitespace and line endings. UTF-8 bytes
are preserved. Unmatched inline markers remain literal.

Links may use relative destinations, fragments, HTTP(S), or mailto. Other schemes,
control characters, spaces, and backslashes leave the entire link literal. URLs
are also HTML-escaped. The HTML template is trusted site-owned markup; Markdown
and titles always go through the proved escaper. Template placeholders are
substituted in one pass, so placeholders within input text remain text.

The fixtures compare exact HTML fragments using the actual executable. The shell
driver also checks title selection, escaping, template collisions, CRLF/EOF,
empty input, manifest handling and I/O failures in a temporary directory.

See [REPORT.md](REPORT.md) for the development experience and verification notes.
