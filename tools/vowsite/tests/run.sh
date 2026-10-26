#!/bin/sh
set -eu
# The production executable is exercised in an isolated working directory.
# No extra Vow entry point, proc effect, or alternative parser is needed.
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
binary=${VOWSITE:-"$root/build/vowsite"}
if [ ! -x "$binary" ]; then
  make -C "$root" build/vowsite
fi
work=$(mktemp -d "${TMPDIR:-/tmp}/vowsite-tests.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
ulimit -c 0
mkdir -p "$work/site" "$work/public"
cd "$work"
printf '%s' '{{content}}' > site/template.html
: > site/pages.txt
count=0
for source in "$root"/tools/vowsite/tests/*.md; do
  name=$(basename "$source" .md)
  cp "$source" "site/$name.md"
  printf '%s\n' "$name" >> site/pages.txt
done
"$binary" > generated.log
for expected in "$root"/tools/vowsite/tests/*.html; do
  name=$(basename "$expected")
  diff -u "$expected" "public/$name"
  count=$((count + 1))
done
printf '%s\n' "$count Markdown fixtures passed"

printf 'sample\n' > site/pages.txt
cat > site/sample.md <<'EOF'
```
# False title
```
# Real <title> & "quote" {{content}}
# Later title
{{title}} {{content}}
EOF
printf '%s' '<title>{{title}}</title>{{content}}<aside>{{title}}</aside>' > site/template.html
"$binary" > generated.log
cat > expected.html <<'EOF'
<title>Real &lt;title&gt; &amp; &quot;quote&quot; {{content}}</title><pre><code># False title
</code></pre>
<h1>Real &lt;title&gt; &amp; &quot;quote&quot; {{content}}</h1>
<h1>Later title</h1>
<p>{{title}} {{content}}</p>
EOF
printf '%s' '<aside>Real &lt;title&gt; &amp; &quot;quote&quot; {{content}}</aside>' >> expected.html
diff -u expected.html public/sample.html

# CRLF, exact code whitespace, and EOF without a newline.
printf '%s' '{{content}}' > site/template.html
printf '# CRLF\r\n\r\n```\r\n\t <&>\r\n```\r\n\r\nlast' > site/sample.md
"$binary" > generated.log
printf '<h1>CRLF</h1>\n<pre><code>\t &lt;&amp;&gt;\r\n</code></pre>\n<p>last</p>\n' > expected.html
diff -u expected.html public/sample.html
printf '```\n  last <&>' > site/sample.md
"$binary" > generated.log
printf '<pre><code>  last &lt;&amp;&gt;</code></pre>\n' > expected.html
diff -u expected.html public/sample.html

# Empty input, whitespace-only lines, and the page-name title fallback.
: > site/sample.md
printf '%s' '{{title}}:{{content}}' > site/template.html
"$binary" > generated.log
printf 'sample:' > expected.html
diff -u expected.html public/sample.html
printf 'one\n \t \nsecond\n' > site/sample.md
"$binary" > generated.log
printf 'sample:<p>one</p>\n<p>second</p>\n' > expected.html
diff -u expected.html public/sample.html

# Manifest whitespace and CRLF are accepted; no trailing newline is necessary.
printf '\r\n sample \r\n\t\r\nother' > site/pages.txt
printf '# Other' > site/other.md
"$binary" > generated.log
test -f public/other.html
printf 'Other:<h1>Other</h1>\n' > expected.html
diff -u expected.html public/other.html

expect_failure() {
  message=$1
  if ("$binary" > failure.log 2>&1) 2>/dev/null; then
    printf 'FAIL: expected failure: %s\n' "$message" >&2
    exit 1
  fi
  if ! grep -qF "$message" failure.log; then
    cat failure.log >&2
    exit 1
  fi
}

printf '../escape\n' > site/pages.txt
expect_failure 'invalid page name'
test ! -e escape.html
printf 'sample\nsample\n' > site/pages.txt
expect_failure 'duplicate page name'
printf 'missing\n' > site/pages.txt
expect_failure 'cannot read site/missing.md'
printf 'sample\n' > site/pages.txt
mv site/template.html template.saved
expect_failure 'cannot read site/template.html'
mv template.saved site/template.html
mv site/pages.txt pages.saved
expect_failure 'cannot read site/pages.txt'
mv pages.saved site/pages.txt
mv public public.saved
expect_failure 'cannot write public/sample.html'
mv public.saved public

# An empty manifest is a successful no-op.
: > site/pages.txt
"$binary" > generated.log
test ! -s generated.log

# Pages from any path, a per-page template, @base, and links between directories.
mkdir -p docs
printf '# Guide\n[spec](SPEC.md) [roadmap](../ROADMAP.md) [code](../src/x.vow)\n' > docs/GUIDE.md
printf '# Spec\n' > docs/SPEC.md
printf '# Roadmap\n[guide](docs/GUIDE.md#top)\n' > ROADMAP.md
printf '%s' '[home]{{content}}' > site/home.html
printf '%s' '{{content}}' > site/template.html
printf '@base https://example.com/blob/\nguide docs/GUIDE.md site/home.html\nspec docs/SPEC.md\nroadmap ROADMAP.md\n' > site/pages.txt
"$binary" > generated.log
printf '[home]<h1>Guide</h1>\n<p><a href="spec.html">spec</a> <a href="roadmap.html">roadmap</a> <a href="https://example.com/blob/src/x.vow">code</a></p>\n' > expected.html
diff -u expected.html public/guide.html
printf '<h1>Roadmap</h1>\n<p><a href="guide.html#top">guide</a></p>\n' > expected.html
diff -u expected.html public/roadmap.html
printf '14 integration checks passed\n'
