# Vow for LLMs

This guide is for language models writing Vow. Read it once before you start. It
covers what differs from the languages you know best, the rules that cause most
compile errors, how to make proofs go through, and the idioms that work. Every code
pattern here comes from verified Vow code (most of it under `examples/`) that the test suite
verifies.

The full grammar and semantics are in [`SPEC.md`](spec.html). When this guide and the
spec disagree, the spec wins.

## 1. The model

- Vow compiles to C. `vowc check` type-checks **and proves** your program with Z3. If a proof fails, the program does not compile.
- What is proved: list and string indices are in bounds, divisors are non-zero, `slice` ranges are valid, `char` codes are bytes, every refinement (`Int where it >= 0`) holds where a value enters a refined position, every `requires` holds at each call, every `ensures` holds at each `return`, every `assert`, and every loop `invariant` (on entry, after each iteration, and at `break` and `continue`).
- Termination is proved where you give a `decreases` measure (section 4); otherwise contracts are proved assuming the code returns. The absence of integer overflow is not proved (overflow traps at runtime instead).
- Every value is immutable. `var` + `set` reassign a local; `inout` parameters let a function hand a new value back to the caller's var. There is no aliasing.

Commands:

```
vowc check file.vow            # type-check and verify
vowc run file.vow -- args      # verify, compile, run
vowc build file.vow -o prog    # verify and compile
vowc check file.vow --json     # one JSON diagnostic per line (for tools)
```

## 2. Rules that cause most errors

Check your code against this list before running the compiler.

1. **No operator precedence.** An expression may not mix different binary operators without parentheses: write `a + (b * c)`, `(i + 1) < n`, `(x > 0) and (y > 0)`. Only `+ * ++ and or` may chain with *themselves* (`a + b + c`). `a - b - c` and `a < b < c` are errors.
2. **Every block has a labelled end.** `end fn name`, `end struct Name`, `end enum Name`, `end if`, `end while`, `end for`, `end match`. `elif` (not `else if`).
3. **Arguments are named.** `f(x: 1, y: 2)`. Only a function with exactly one parameter may be called positionally: `len(xs)`, `print(s)`.
4. **Results cannot be ignored.** A call whose result is not `Unit` must be bound, or explicitly discarded: `let ok = write_file(path: p, contents: c)` or `let _ = advance(inout p)`.
5. **No shadowing.** A name may not be declared again while it is in scope, including in nested blocks and loop variables. Pick a new name.
6. **Keywords can be labels but not names.** A field, argument label, `.field` or pattern field may be a keyword (`d.type`, `f(in: x)`); a variable, parameter or binder may not. The keywords are: `fn end struct enum type let var set return if then elif else while for in do match case invariant requires ensures uses assert check break continue and or not implies true false with import it result where inout forall exists decreases`.
7. **`==` and `!=` work on `Int`, `Bool` and `String` only.** For enums, use `match`.
8. **Empty literals need a type.** `[]`, `{}` and `Option.None` need an annotation or an expected type: `var xs: List<Int> = []`, `let none: Option<Ty> = Option.None`.
9. **Construction is explicit.** Structs: `Point(x: 1, y: 2)`. Variants: `Shape.Rect(w: 1, h: 2)`, `Shape.Empty`, `Option.Some(value: 3)`. Patterns use the bare variant name: `case Rect(w: w, h: h) then`.
10. **`match` must be exhaustive.** On an enum, cover every variant *or* add `else`, but not both. On `Int` or `String`, `else` is required. An empty arm is fine: `case None then` followed by the next `case`.
11. **One global namespace.** `import "file.vow"` merges files. Function and type names must be unique across all imported files.
12. **No generics in user code.** Only `List`, `Map` and `Option` are generic. Write monomorphic helpers (`contains_str`, `int_list_contains`).
13. **No floats yet.** `/` and `%` are integer division with Euclidean semantics: the remainder is never negative (`-7 / 2 == -4`, `-7 % 2 == 1`).
14. **Statements start with a keyword or a call.** `x = 1` is not a statement; use `set x = 1`. A bare expression that is not a call is an error.
15. **`exit` and `panic` end a path.** Code after them in the same block is an "unreachable statement" error.
16. **Character literals are `Int`s.** `'a'`, `'\n'` and `'\''` are byte values, and `s[i]` on a `String` is the byte at `i`.

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
construction and `with` update, and assumed at every read. A cursor that is always
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

**Sortedness: use the pairwise form.** `forall i in 0..n: forall j in i..n: xs[i] <=
xs[j]` is what proofs need; the adjacent form (`xs[i-1] <= xs[i]`) needs induction,
which the verifier cannot do.

**Range-loop invariants may mention the loop variable.** In `for i in 0..n`, an
invariant about `i` holds with `i = 0` on entry and `i + 1` after each iteration, and
after the loop `i = n`. So `invariant len(out) == i` gives `len(out) == n` afterwards.

**Prove termination with `decreases`.** For a `while` loop, give a measure that is
non-negative at the top of an iteration and smaller at its end:
`while lo < hi invariant ... decreases hi - lo do`. For a recursive function, put
`decreases n` in its contract. If the measure needs help, add a progress invariant
(`let start = i` before the loop, `invariant i > start` inside). Without a measure, a
contract only says what holds *if* the code returns.

**Specifications are restricted.** Inside `requires`, `ensures`, `where`, `invariant`,
`decreases`, `assert` and `check` you may use literals, variables, `it`, `result`,
arithmetic, comparisons, `and or not implies`, quantifiers, `len`, `value_or`,
`is_some`, `is_none`, field access, indexing and expression functions. Other calls
are rejected.

**Escape hatches, in order of preference:**
1. Add the missing fact as an invariant, `ensures`, or refinement.
2. Restructure with a guard (`if i < len(xs) then ...`) so the fact is local.
3. `assert P`: proved, then assumed afterwards. Useful as a stepping stone.
4. `check P`: tested at runtime (panics if false), then assumed. Use it only for facts that are true but beyond the verifier, and say why in a comment. The self-hosted compiler uses no `check` at all.

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

## 6. Idioms

**Mutable state through `inout`** (from the self-hosted compiler):

```
fn line(g: inout Gen, s: String) do
  set g = g with (out: push(list: g.out, item: repeat(s: "    ", n: g.ind) ++ s))
end fn line

line(g: inout g, s: "return 0;")       // the caller writes 'inout' too
```

An `inout` argument must be a `var`. The same var cannot be passed `inout` twice in
one call.

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

**Building strings.** Collect parts in a `List<String>` and `join` once at the end.
Repeated `++` copies the whole string every time.

**Removing the last list element.** `slice(of: xs, from: 0, to: len(xs) - 1)` after
checking `len(xs) > 0`.

**A loop that runs at least once, or a loop with a flag:**

```
var more = true
while more do
  set items = push(list: items, item: parse_item(inout p))
  set more = accept(p: inout p, text: ",")
end while
```

**Errors without exceptions.** Return an `Option`, a struct with an `error: String`
field (empty means success), or record diagnostics in an `inout` state. The
self-hosted parser records the first error and jumps its cursor to end of file, so
every loop stops naturally.

**Effects.** Declare the smallest set you need: `uses console` for
`print`/`eprint`, `uses fs` for files, `uses proc` for `args`, `getenv`, `run` and
`exit`. A pure function needs no `uses` clause, and that is checkable by anyone
reading its signature.

## 7. A working session

1. Write signatures first, with the contracts callers will rely on.
2. Write bodies. Use `for x in xs` and `for i in 0..len(xs)` where you can.
3. Run `vowc check`. Fix parse and type errors (usually rules 1, 3, 4, 5, 6).
4. For each proof failure: read the obligation and the counterexample, then add the missing invariant, `ensures` or guard.
5. Re-run. Unchanged functions are cached and skipped, so iteration is fast.

## 8. Larger examples

- `examples/bsearch.vow`: full functional specification (sorted input, found index holds the target, -1 means absent) and termination
- `examples/isort.vow`: insertion sort proved sorted and length-preserving
- `examples/escape.vow`: an HTML escaper proved never to emit `<` or `>`
- `examples/cursor.vow`: refined struct fields with `inout`
- `examples/wordfreq.vow`: string scanning, maps and options
- `examples/shapes.vow`: enums, refinement aliases, exhaustive `match`
