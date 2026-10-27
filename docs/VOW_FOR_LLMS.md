# Vow for LLMs

This guide is for language models writing Vow. Read it once before you start. It
covers what differs from the languages you know best, the rules that cause most
compile errors, how to make proofs go through, and the idioms that work. Every code
pattern here comes from a file under `examples/` or `compiler/` that the test suite
verifies.

The full grammar and semantics are in [`SPEC.md`](SPEC.md). When this guide and the
spec disagree, the spec wins.

## 1. The model

- Vow compiles to C. `vowc check` type-checks **and proves** your program with Z3.
  If a proof fails, the program does not compile.
- What is proved: list and string indices are in bounds, divisors are non-zero,
  `slice` ranges are valid, `char` codes are bytes, every refinement (`Int where
  it >= 0`) holds where a value enters a refined position, every `requires` holds at
  each call, every `ensures` holds at each `return`, every `assert`, and every loop
  `invariant` (on entry, after each iteration, and at `break` and `continue`).
- Termination is proved where you give a `decreases` measure (section 4); otherwise
  contracts are proved assuming the code returns. The absence of integer overflow is
  not proved (overflow traps at runtime instead).
- Every value is immutable. `var` + `set` reassign a local; `inout` parameters let a
  function hand a new value back to the caller's var. There is no aliasing.

Commands:

```
vowc check file.vow            # type-check and verify
vowc run file.vow -- args      # verify, compile, run
vowc build file.vow -o prog    # verify and compile
vowc check file.vow --json     # one JSON diagnostic per line (for tools)
vowc fmt file.vow              # rewrite in the canonical layout (whitespace only)
vowc test file_test.vow        # verify, then run each test_... function
```

## 2. Rules that cause most errors

Check your code against this list before running the compiler.

1. **No operator precedence.** An expression may not mix different binary operators
   without parentheses: write `a + (b * c)`, `(i + 1) < n`, `(x > 0) and (y > 0)`.
   Only `+ * ++ and or` may chain with *themselves* (`a + b + c`). `a - b - c` and
   `a < b < c` are errors.
2. **Every block has a labelled end.** `end fn name`, `end struct Name`,
   `end enum Name`, `end if`, `end while`, `end for`, `end match`. `elif` (not
   `else if`).
3. **Arguments are named.** `f(x: 1, y: 2)`. Only a function with exactly one
   parameter may be called positionally: `len(xs)`, `print(s)`.
4. **Results cannot be ignored.** A call whose result is not `Unit` must be bound,
   or explicitly discarded: `let ok = write_file(path: p, contents: c)` or
   `let _ = advance(inout p)`.
5. **No shadowing.** A name may not be declared again while it is in scope, including
   in nested blocks and loop variables. Pick a new name.
6. **Keywords can be labels but not names.** A field, argument label, `.field` or
   pattern field may be a keyword (`d.type`, `f(in: x)`); a variable, parameter or
   binder may not. The keywords are:
   `fn end struct enum type let var set return if then elif else while for in do
   match case invariant requires ensures uses assert check break continue and or not
   implies true false with import it result where inout forall exists decreases try`.
7. **`==` and `!=` compare structurally**: strings by bytes, lists element by element,
   structs field by field, enums and options by variant and fields. Values that are or
   contain maps cannot be compared. To test only which variant a value is, use
   `match` (or `is_some`/`is_none`).
8. **Empty literals need a type.** `[]`, `{}`, `Option.None` and variants of generic
   enums that don't fix every type parameter (`Result.Err(error: e)`, `Tree.Leaf`)
   need an annotation or an expected type: `var xs: List<Int> = []`,
   `let none: Option<Ty> = Option.None`. A `return` or an argument supplies one.
9. **Construction is explicit.** Structs: `Point(x: 1, y: 2)`. Variants:
   `Shape.Rect(w: 1, h: 2)`, `Shape.Empty`, `Option.Some(value: 3)`. Patterns use
   the bare variant name: `case Rect(w: w, h: h) then`.
10. **`match` must be exhaustive.** On an enum, cover every variant *or* add `else`,
    but not both. On `Int` or `String`, `else` is required. An empty arm is fine:
    `case None then` followed by the next `case`.
11. **One global namespace.** `import "file.vow"` merges files. Function and type names
    must be unique across all imported files.
12. **Generic type arguments are inferred, never written at calls.** Declare
    `fn last<T>(xs: List<T>) -> T` and call `last(xs)`; write types in full
    (`Pair<Int, String>`). Inside a generic function, a `T` can be stored, passed,
    returned, compared with `==` and put in lists and maps, but not indexed or given
    fields, and generic functions cannot be called in specifications.
13. **No floats yet.** `/` and `%` are integer division with Euclidean semantics: the
    remainder is never negative (`-7 / 2 == -4`, `-7 % 2 == 1`).
14. **Statements start with a keyword or a call.** `x = 1` is not a statement; use
    `set x = 1`. A bare expression that is not a call is an error.
15. **`exit` and `panic` end a path.** Code after them in the same block is an
    "unreachable statement" error.
16. **Character literals are `Int`s.** `'a'`, `'\n'` and `'\''` are byte values, and
    `s[i]` on a `String` is the byte at `i`.

## 3. Syntax at a glance

```
import "util.vow"

type Nat = Int where it >= 0

struct Point
  x: Int,
  y: Int where it >= x,          // may mention sibling fields
end struct Point

enum Shape
  Square(side: Nat),
  Empty,
end enum Shape

fn area(s: Shape) -> Nat do
  match s
  case Square(side: n) then
    return n * n
  case Empty then
    return 0
  end match
end fn area

fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires lo <= hi
  ensures (lo <= result) and (result <= hi)
do
  if x < lo then
    return lo
  elif x > hi then
    return hi
  end if
  return x
end fn clamp

fn main() uses console do
  var total = 0
  for i in 0..10 invariant total >= 0 do
    set total = total + i
  end for
  print("total " ++ int_to_str(total) ++ "\n")
end fn main
```

Built-ins: `len push slice join int_to_str str_to_int char insert lookup has keys hash
panic print eprint read_file write_file args getenv run exit`. Signatures are in the
spec. `print` adds no newline.

### The standard library

`import "std/text.vow"` works from any file (the module is built into the compiler).
Every function in it is verified and carries a contract, so use it rather than writing
your own string helpers:

| Function | What its contract tells you |
|---|---|
| `is_digit(c)`, `is_alpha(c)`, `is_alnum(c)`, `is_space(c)` | transparent (expression functions) |
| `starts_at(s: s, at: i, p: p)`, `starts_with(s: s, p: p)`, `ends_with(s: s, p: p)` | transparent: when true, the bounds and every byte of the match are known |
| `find(s: s, p: p, from: i) -> Int` | `-1`, or an index `>= from` where `p` occurs; `-1` means no occurrence at or after `from` |
| `find_byte(s: s, c: c, from: i)`, `find_last_byte(s: s, c: c)` | `-1` or an in-bounds index holding `c`; `-1` means absent |
| `contains(s: s, p: p) -> Bool` | true implies an occurrence exists |
| `split(s: s, sep: c) -> List<String>` | at least one piece; no piece contains `sep` |
| `lines(s) -> List<String>` | handles `\n` and `\r\n`; no line contains `\n` |
| `trim(s)`, `trim_start(s)`, `trim_end(s)` | no longer than `s`; no surrounding space |
| `replace(s: s, old: o, new: n)`, `repeat(s: s, n: k)` | proved to terminate |
| `to_lower(s)`, `to_upper(s)` | same length; no upper (lower) case letters |
| `compare(a: x, b: y) -> Int` | byte-wise order: -1, 0 or 1 (strings have no `<`) |

Library names share the global namespace, so don't define your own `trim` in a file
that imports it.

## 4. Making proofs go through

The verifier reasons about each function **using only the signatures of the functions
it calls**. A callee's body is invisible to its callers, so anything a caller needs
must be in the callee's `ensures` or return refinement.

**Index safely.** The verifier must know `0 <= i` and `i < len(xs)` at the access.
These all work:

```
for i in 0..len(xs) do ... xs[i] ... end for          // bounds come from the range
if (i < len(s)) and (s[i] == ' ') then ...           // 'and' short-circuits: s[i] sees i < len(s)
while (i < n) and is_alnum(src[i]) invariant start <= i invariant i <= n do
```

`for x in xs` needs no index at all, so prefer it when you don't need the index.

**Loops forget.** At the loop head, every variable the body assigns (with `set`, or by
passing it `inout`) is replaced by an unknown value. Only the loop invariants and the
variables' declared refinements survive. If you need `i >= 0` inside the loop, say so:

```
var i = 0
while i < len(s) invariant i >= 0 do
  let c = s[i]          // needs 0 <= i (invariant) and i < len(s) (condition)
  set i = i + 1
end while
```

**After a loop**, you know the invariants, plus the negated condition *only if the
loop has no `break`*. A `break` exit keeps only the invariants. So if you need a fact
after the loop, make it an invariant. This is the mistake that cost me most often:

```
// Wrong: after the first loop nothing says a <= b, so slice(a, b) fails.
while (a < b) and is_space(s[a]) invariant 0 <= a invariant b <= len(s) do
// Right:
while (a < b) and is_space(s[a]) invariant 0 <= a invariant a <= b invariant b <= len(s) do
```

**Invariants must hold on entry, at the end of each iteration, and at every `break`
and `continue`.** If an invariant fails "on entry", it is false before the loop.

**Return what callers need.** A helper that returns an index should say so:

```
fn index_of_str(xs: List<String>, x: String) -> Int
  ensures result >= -1
  ensures result < len(xs)
do ...
```

Then a caller can write `let i = index_of_str(...)` and `if i >= 0 then xs[i]`.

**Refine struct fields to carry invariants.** A field refinement is checked at every
construction and `with` update, and assumed at every read, in code and in
specifications (`assert c.pos < len(c.toks)` holds for any `Cursor`). A cursor that is always
valid (from `examples/cursor.vow`):

```
struct Cursor
  toks: List<String> where len(it) > 0,
  pos: Int where (0 <= it) and (it < len(toks)),
end struct Cursor

fn peek(c: Cursor) -> String do
  return c.toks[c.pos]                      // proved from the field refinements
end fn peek

fn advance(c: inout Cursor) -> String do
  let t = c.toks[c.pos]
  if (c.pos + 1) < len(c.toks) then
    set c = c with (pos: c.pos + 1)         // obligation: the new pos is in range
  end if
  return t
end fn advance
```

**Refined parameters move obligations to the caller.** `fn hex_digit(n: Int where
(0 <= it) and (it < 16))` makes every caller prove the range, and lets the body use
it freely.

**Quantify over ranges.** `forall i in a..b: P` and `exists i in a..b: P` work in any
specification (and at runtime, as loops). The verifier knows how `push`, `++`, `slice`
and literals relate the elements or bytes of their results to their inputs, so
element-wise invariants go through. From `examples/escape.vow`:

```
fn tag_free(s: String) -> Bool do
  return forall i in 0..len(s): (s[i] != '<') and (s[i] != '>')
end fn tag_free

fn escape(s: String) -> String ensures tag_free(result) do
  var out = ""
  for i in 0..len(s) invariant tag_free(out) do
    ...
      set out = out ++ "&lt;"
    ...
  end for
  return out
end fn escape
```

**Name your predicates with expression functions.** A pure, non-recursive function
whose whole body is `return <spec expression>` can be used inside specifications
(`tag_free` and `is_sorted` above). It is inlined, and its calls are transparent to the
verifier at runtime too, so `if is_sorted(xs) then ...` teaches the verifier that
`xs` is sorted.

**Recursive functions in contracts.** A pure function whose body uses only `let`,
`if`, `match`, `assert` and `return` can be used in specifications, recursive ones too
when they have a `decreases` measure. This lets a contract say what a recursive
walk produces:

```
fn size(t: Tree) -> Int ensures result >= 0 decreases t do
  match t
  case Leaf then
    return 0
  case Node(left: l, value: v, right: r) then
    return (size(l) + 1) + size(r)
  end match
end fn size

fn in_order(t: Tree, out: inout List<Int>)
  ensures len(out) == (len(old(out)) + size(t))
  decreases t
do ...
```

The verifier unfolds such a function at most twice per use. To prove something about
a long chain (`sum(xs, 0)` for a long list), carry it in a loop invariant
(`invariant s == sum(xs: xs, from: i)`) rather than asking for it all at once.

**Give every helper the contract its callers need.** The verifier reasons about a
call from the callee's signature alone. A single-expression function is transparent;
anything else exposes only its `ensures`. Two cases catch people out:
- A predicate with a loop says nothing when it returns `true` unless you write
  `ensures result implies ...` (see `find` in `std/text.vow`). Prefer writing the
  predicate as one expression, so it is transparent.
- A helper that builds a struct is *not* an expression function (constructors are not
  specification expressions), so its callers learn nothing about the fields. Say what
  they need (from `examples/calc.vow`):

  ```
  fn fail(pos: Int) -> Parsed
    ensures (not result.ok) and (result.pos == pos)
  do
    return Parsed(ok: false, value: 0, pos: pos)
  end fn fail
  ```

**Sortedness: use the pairwise form.** `forall i in 0..n: forall j in i..n: xs[i] <=
xs[j]` is what proofs need; the adjacent form (`xs[i-1] <= xs[i]`) needs induction,
which the verifier cannot do.

**Range-loop invariants may mention the loop variable.** In `for i in 0..n`, an
invariant about `i` holds with `i = 0` on entry and `i + 1` after each iteration, and
after the loop `i = n`. So `invariant len(out) == i` gives `len(out) == n` afterwards.

**Prove termination with `decreases`.** For a `while` loop, give a measure that is
non-negative at the top of an iteration and smaller at its end:
`while lo < hi invariant ... decreases hi - lo do`. For a recursive function, put
`decreases n` in its contract. For recursion over a tree or list, the measure can be
the value itself: `decreases t` proves that recursing on the fields of a `match` arm
(or on `xs[i]`) terminates. Functions that call each other each need a measure, and
each call must make it smaller. If the measure needs help, add a progress invariant
(`let start = i` before the loop, `invariant i > start` inside). Without a measure, a
contract only says what holds *if* the code returns. `vowc check --totality` lists the
functions not proved to terminate, and why.

**Nested loops need progress invariants.** A loop's measure must drop on every path,
including paths through inner loops. After an inner loop the verifier only knows the
inner invariants, so record where it started:

```
let start = i                        // entered on a digit
while (i < len(s)) and is_digit(s[i]) invariant (start <= i) and (i <= len(s)) decreases len(s) - i do
  set i = i + 1
end while
// The verifier now knows i > start: had the loop stopped at once, s[start] would be a
// digit and the condition would still hold.
```

**Recursive descent with measures.** `examples/calc.vow` parses nested parentheses
with one directly recursive function. It is proved to terminate with
`decreases len(s) - pos` because every recursive call starts after a consumed `(`,
and its `ensures (pos <= result.pos) and (result.pos <= len(s))` lets the caller's loop
prove progress.

**Proofs about built strings.** The verifier tracks the bytes of strings built with
`++`, `slice` and `char`, so a property such as "no `<` anywhere" can be an invariant
of a `++` loop (`examples/escape.vow`). It knows nothing about the bytes of a `join`.
So when you need a proof about a string's contents, build that string with `++`. Use
`join` when only speed matters; it avoids the quadratic copying of repeated `++`. A
common split: prove each small piece (for example, every escaped text fragment), then
`join` the pieces into the page.

**Specifications are restricted.** Inside `requires`, `ensures`, `where`, `invariant`,
`decreases`, `assert` and `check` you may use literals, variables, `it`, `result`,
arithmetic, comparisons, `and or not implies`, quantifiers, `len`, `value_or`,
`is_some`, `is_none`, field access, indexing and specification functions (above).
Other calls are rejected.

**Escape hatches, in order of preference:**
1. Add the missing fact as an invariant, `ensures`, or refinement.
2. Restructure with a guard (`if i < len(xs) then ...`) so the fact is local.
3. `assert P`: proved, then assumed afterwards. Useful as a stepping stone.
4. `check P`: tested at runtime (panics if false), then assumed. Use it only for
   facts that are true but beyond the verifier, and say why in a comment. The
   self-hosted compiler uses no `check` at all.

## 5. Reading the compiler's output

Proof failures name the obligation and, when Z3 finds one, a counterexample over the
`Int` and `Bool` variables in scope and the lengths of lists, strings and maps:

```
tests/fail/invariant.vow:5:25: error: cannot prove loop invariant is preserved: total <= i
    counterexample: i = 1, n = 1, total = 2
    in function 'sum_to'
```

The counterexample shows the values at the failing point. Here, after one iteration
`total` is 2 and `i` is 1. Ask whether the claim is false (fix the code or the
invariant) or merely unproved (add a fact).

No counterexample means Z3 found the claim unprovable from what it knows, but
couldn't present values for the variables involved:

```
tests/fail/pre.vow:6:11: error: cannot prove precondition of 'half': (n % 2) == 0
```

"Z3 gave up" means a timeout or nonlinear arithmetic. Split the step, or add an
intermediate `assert`.

**Holes** let you ask the compiler what a spot needs. Write `?name` where an
expression goes:

```
/tmp/hole.vow:8:12: hole: ?upper needs a value of type Int
    in scope: x: Int, lo: Int, hi: Int
    known facts: lo <= hi; not (x < lo); x > hi
```

Use holes to get unstuck, or to hand a decision back to a human: a program with holes
never compiles, so it can't be shipped by accident.

`--json` prints one object per diagnostic:

```
{"kind":"error","file":"tests/fail/index.vow","line":3,"col":10,"message":"cannot prove index in bounds: 0 <= 0 < len(xs)","notes":["counterexample: len(xs) = 0","in function 'first'"]}
```

`kind` is `"error"` or `"hole"`.

**Runtime panics** name the source line: `vow: panic at calc.vow:12: integer overflow
in '*'` (exit status 101). In a verified program the only runtime failures are integer
overflow, `panic(msg)` and a failed `check`; bounds and division are proved. `vowc check
file.vow --totality` lists the functions not proved to terminate, with the reason.

## 6. Idioms

**Mutable state through `inout`** (from `compiler/codegen.vow`):

```
fn line(g: inout Gen, s: String) do
  set g = g with (out: push(list: g.out, item: repeat(s: "    ", n: g.ind) ++ s))
end fn line

line(g: inout g, s: "return 0;")       // the caller writes 'inout' too
```

An `inout` argument must be a `var`. The same var cannot be passed `inout` twice in
one call.

To say how an `inout` parameter changes, use `old(x)` (its value on entry) in
`ensures` and loop invariants; otherwise callers learn only its refinement:

```
fn bump(c: inout Counter)
  ensures c.n == (old(c).n + 1)
  ensures c.name == old(c).name
do
  set c = c with (n: c.n + 1)
end fn bump
```

**Optional values.** `lookup`, `str_to_int`, `read_file` and `getenv` return
`Option<T>`. For a default, use `value_or`; to test, `is_some` / `is_none`:

```
let n = value_or(opt: lookup(map: counts, key: word), default: 0)
set counts = insert(map: counts, key: word, value: n + 1)
```

When both cases need different code, `match`:

```
match lookup(map: counts, key: word)
case Some(value: n) then
  return insert(map: counts, key: word, value: n + 1)
case None then
  return insert(map: counts, key: word, value: 1)
end match
```

**Errors: `Result` and `try`.** Every program has
`enum Result<T, E> Ok(value: T), Err(error: E) end enum Result`. In a function that
returns a `Result`, `let x = try f(...)` binds the `Ok` value, or returns the `Err`
from the enclosing function unchanged (so the error types must match). In a function
that returns an `Option`, `try` does the same with `Some` and `None`. `try` may only be
the whole value of `let`, `var` or `set`, or a call statement. From
`examples/results.vow`:

```
fn parse_line(text: String, line: Int) -> Result<Setting, String> do
  let eq = find_byte(s: text, c: '=', from: 0)
  if eq < 0 then
    return Result.Err(error: "line " ++ int_to_str(line) ++ ": expected 'name = value'")
  end if
  let name = trim(slice(of: text, from: 0, to: eq))
  let count = try parse_count(text: slice(of: text, from: eq + 1, to: len(text)), line: line)
  return Result.Ok(value: Setting(name: name, value: count.value))
end fn parse_line
```

Contracts cannot look inside a `Result` (or an `Option`). When callers need a fact
about the `Ok` value, return a struct with a refined field (`Result<Count, String>`
where `Count` has `value: Int where it >= 0`): reading `count.value` gives the
verifier the refinement.

A function that can only succeed or fail returns `Result<Unit, E>`, writes
`return Result.Ok` (no value), and its callers write `try check(...)` as a statement.

**Generic helpers.** Write a helper once for every element type:

```
fn last_or<T>(xs: List<T>, default: T) -> T do
  if len(xs) == 0 then
    return default
  end if
  return xs[len(xs) - 1]
end fn last_or
```

A generic function is verified once, for all types, and compiled separately for each
type it is used with, so it costs nothing at runtime.

**Building strings.** Collect parts in a `List<String>` and `join` once at the end.
Repeated `++` copies the whole string every time.

**Performance: update in place with `set x = op(x, ...)`.** Values are immutable, but
when a value has only one reference, `push`, `set_at`, `with` and `insert` update it in
place. Write updates as `set xs = set_at(list: xs, at: i, value: v)`,
`set s = s with (count: s.count + 1)` or `set m = insert(map: m, key: k, value: v)`:
the old value is moved into the operation, so nothing is copied. A second reference
(`let copy = xs`) makes the next update copy once, which is what keeps `copy`
unchanged. The same holds for a field: in `set s = s with (items: push(list: s.items, item: x))`
the list is moved out of `s` and updated in place, as long as the right-hand side reads
`s.items` only once.

**`Map` or `HashMap`.** Both have the same operations (`insert`, `lookup`, `has`,
`keys`, `len`, `{}`), and `keys` is sorted for both. Use `HashMap` for counting and
lookup tables that are built up and then read; it is several times faster. Use `Map`
when you keep old versions around (`let before = m` and then insert), because a shared
`HashMap` is copied whole on the next insert, while a shared `Map` copies only one path.

**Removing the last list element.** `slice(of: xs, from: 0, to: len(xs) - 1)` after
checking `len(xs) > 0`.

**A loop with a flag:**

```
var more = true
while more do
  set items = push(list: items, item: parse_item(inout p))
  set more = accept(p: inout p, text: ",")
end while
```

This loop has no `decreases` measure, so it gets no termination proof. That's fine
when termination isn't required. When it is, loop over a quantity that shrinks, such
as the remaining input (`while pos < len(toks) ... decreases len(toks) - pos`).

**Text processing.** Trim, skip blanks and comments, split on a separator, and let
the library's contracts prove the slices (from `examples/config.vow`):

```
import "std/text.vow"

fn parse_line(line: String) -> Option<Entry> do
  let t = trim(line)
  if (len(t) == 0) or starts_with(s: t, p: "#") then
    return Option.None
  end if
  let eq = find_byte(s: t, c: '=', from: 0)
  if eq < 0 then
    return Option.None
  end if
  // find_byte's contract says t[eq] is '=', so both slices are in bounds.
  let key = trim(slice(of: t, from: 0, to: eq))
  ...
```

`for line in lines(text)` handles both `\n` and `\r\n` line endings.

**Errors without exceptions.** Return an `Option`, a struct with an `error: String`
field (empty means success), or record diagnostics in an `inout` state. The
self-hosted parser records the first error and jumps its cursor to end of file, so
every loop stops naturally.

**Effects.** Declare the smallest set you need: `uses console` for
`print`/`eprint`, `uses fs` for files and directories (`read_file`, `write_file`,
`make_dir`, `list_dir`), `uses proc` for `args`, `getenv`, `run` and `exit`. A pure
function needs no `uses` clause, and that is checkable by anyone reading its signature.

**A command-line tool without `proc`.** Keep every function pure except `main`, and
end on errors with `panic`, which needs no effect. `exit` and `args` need `proc`, so
a tool limited to `fs` and `console` uses fixed paths or reads its inputs from a file.
`make_dir` creates missing parents like `mkdir -p`; `list_dir` returns sorted names.
From `examples/mkindex.vow`:

```
fn main() uses fs, console do
  match list_dir("examples")
  case None then
    panic("cannot list examples (run from the repository root)")
  case Some(value: names) then
    if not make_dir("build/index") then
      panic("cannot create build/index")
    end if
    ...
```

## 7. A working session

1. Write signatures first, with the contracts callers will rely on.
2. Write bodies. Use `for x in xs` and `for i in 0..len(xs)` where you can.
3. Run `vowc check`. Fix parse and type errors (usually rules 1, 3, 4, 5, 6).
4. For each proof failure: read the obligation and the counterexample, then add the
   missing invariant, `ensures` or guard.
5. Re-run. Unchanged functions are cached and skipped, so iteration is fast.
6. Write tests where proofs stop: `fn test_name()` functions, with `check` for what
   holds at runtime (bind calls with `let` first: `check` takes a specification) and
   `assert` for what the verifier proves. `vowc test file.vow` verifies the file, then
   runs each test in its own process and shows the output of any that fail.
7. Run `vowc fmt` on the file before handing it over. It changes only whitespace (it
   checks that the tokens and comments are unchanged), so it is always safe.

## 8. Larger examples

- `examples/bsearch.vow`: full functional specification (sorted input, found index
  holds the target, -1 means absent) and termination
- `examples/isort.vow`: insertion sort proved sorted and length-preserving
- `examples/escape.vow`: an HTML escaper proved never to emit `<` or `>`
- `examples/config.vow`: text processing with `std/text.vow`
- `examples/calc.vow`: recursive descent proved to terminate
- `examples/mkindex.vow`: a command-line tool with only `fs` and `console`
- `tools/vowsite/`: a 545-line static site generator, written by another model from
  this guide alone (see its `REPORT.md`)
- `std/text.vow`: the verified standard text library
- `examples/cursor.vow`: refined struct fields with `inout`
- `examples/wordfreq.vow`: string scanning, maps and options
- `examples/shapes.vow`: enums, refinement aliases, exhaustive `match`
- `examples/results.vow`: errors with `Result` and `try`, and a generic helper
- `compiler/`: the self-hosted compiler, about 6,000 lines of verified Vow
