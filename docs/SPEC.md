# Vow Language Specification (v0)

Vow is designed for a machine author. Its guiding rule is that every function is
checkable from its signature alone: effects are declared, contracts are proved by an
SMT solver, and nothing depends on counting or on context far away.

## Design rules

1. **No counting.** No significant whitespace, no operator precedence, every block has a
   labelled end (`end fn parse`, `end if`), so a missing delimiter fails locally.
2. **Names over positions.** Calls use named arguments (`f(x: 1, y: 2)`). A function with
   exactly one parameter may be called positionally: `len(xs)`.
3. **One way to say it.** No overloading, no implicit conversions, no shadowing, no
   global mutable state, no exceptions, no null.
4. **Contracts are proved, not hoped.** Refinement types, `requires`, `ensures`, `assert`
   and loop `invariant`s are discharged by Z3 at compile time. Index bounds, division by
   zero and refinement subtyping are proof obligations. `check` is the one explicit
   runtime-checked escape hatch.
5. **Effects are part of the signature.** A function is pure unless it declares
   `uses console`, `uses fs` or `uses proc`. Callers must declare a superset.
6. **Content-addressed verification.** Each function's verification result is cached
   under a hash of its body plus the contracts of everything it calls.

## Lexical structure

- Comments: `//` to end of line.
- Identifiers: `[A-Za-z_][A-Za-z0-9_]*`. Types, structs, enums and variants are
  capitalised by convention; this is not enforced.
- Integers: decimal digits, `_` allowed as separator. No negative literals (use unary `-`).
- Strings: `"..."` with escapes `\n \t \r \\ \" \0`.
- Characters: `'a'`, `'\n'`, `'\''` are `Int` literals holding one byte.
- Holes: `?name`.
- Newlines are insignificant everywhere.

Keywords:
`fn end struct enum type let var set return if then elif else while for in do match case
invariant requires ensures uses assert check break continue and or not implies true false
with import it result where inout forall exists decreases`

A keyword may be used as a **label**: a struct or variant field name, a named-argument
label, a field access (`d.type`), a `with` field or a pattern field. Keywords may not
name variables, parameters or binders.

## Program structure

A program is a sequence of declarations. `import "path.vow"` textually merges another
file (resolved relative to the importing file; each file is included once). Paths
beginning with `std/` name standard library modules built into the compiler, such as
`import "std/text.vow"`. All
declarations share one global namespace and may appear in any order.

```
import "util.vow"

type Nat = Int where it >= 0

struct Point
  x: Int,
  y: Int where it >= x,      // field refinements may mention sibling fields
end struct Point

enum Shape
  Circle(radius: Nat),
  Rect(w: Nat, h: Nat),
  Empty,
end enum Shape

fn area(s: Shape) -> Nat do
  match s
  case Circle(radius: r) then
    return 3 * (r * r)
  case Rect(w: w, h: h) then
    return w * h
  case Empty then
    return 0
  end match
end fn area
```

Every program has `fn main()` that takes no parameters and returns `Unit`.

## Types

| Type | Meaning |
|---|---|
| `Int` | 64-bit signed integer; overflow traps at runtime |
| `Bool` | `true` / `false` |
| `String` | immutable byte string |
| `Unit` | the empty value; the return type of functions without `->` |
| `List<T>` | immutable list |
| `Map<K, V>` | immutable ordered map; `K` is `Int` or `String` |
| `Option<T>` | built-in enum: `Some(value: T)` and `None` |
| `Output` | built-in struct: `code: Int, out: String` (result of `run`) |
| `T where P` | refinement: values of `T` satisfying predicate `P` over `it` |
| user `struct` / `enum` | nominal; enums may be recursive |

All values are immutable. Local `var` bindings may be reassigned with `set`, and a
function may take `inout` parameters, which gives mutable-value semantics without
aliasing:

```
fn emit(g: inout Gen, line: String) do
  set g = g with (lines: push(list: g.lines, item: line))
end fn emit

emit(g: inout gen, line: "x")      // 'inout' is written at the call site too
```

An `inout` argument must be a `var` (or an `inout` parameter of the caller), and the same
var may not be passed `inout` twice in one call. Semantically the callee receives the
value and the caller's var receives the callee's final value when the call returns.
In `ensures`, an `inout` parameter names its final value; in `requires`, its initial
value. The parameter's refinement must hold whenever it is assigned and is assumed by
the caller afterwards. User code is monomorphic; only the built-in
types are generic.

Refinements may appear on parameter types, return types, `let`/`var` annotations, struct
and variant fields, and type aliases. A parameter refinement may refer to earlier
parameters. A field refinement may refer to sibling fields. Refinements nested inside
`List<...>`, `Map<...>` or `Option<...>` are not allowed; wrap the element in a struct
with a refined field instead.

## Declarations

```
fn name(p1: T1, p2: T2) -> R
  uses console, fs, proc       // optional effect list
  requires <bool expr>         // zero or more
  ensures <bool expr>          // zero or more; may mention `result`
  decreases <int expr>         // optional termination measure for recursion
do
  <statements>
end fn name
```

The return type may be refined: `-> Int where it >= 0`. Inside `ensures`, `result`
names the return value.

A function that is pure (no `uses`), returns a value, has no `inout` parameters, is not
recursive, and whose whole body is `return e` where `e` is a specification expression is
an **expression function**. Expression functions may be called in specifications, where
they are inlined, and their calls are transparent to the verifier everywhere:

```
fn is_sorted(xs: List<Int>) -> Bool do
  return forall i in 0..len(xs): forall j in i..len(xs): xs[i] <= xs[j]
end fn is_sorted

fn search(xs: List<Int>, target: Int) -> Int requires is_sorted(xs) ...
```

`struct` and `enum` are shown above. Fields and variants are comma-separated, with an
optional trailing comma. `type Name = T` declares an alias.

## Statements

```
let x = e                  // immutable binding
let _ = e                  // evaluate and discard
let x: T = e               // annotated (annotation may be refined)
var x = e                  // reassignable binding
set x = e                  // reassign a var
return e                   // or bare `return` in a Unit function
if c then ... elif c then ... else ... end if
while c invariant P decreases M do ... end while   // decreases is optional
for i in a..b invariant P do ... end for     // a <= i < b; a and b evaluated once;
                                             // invariants may mention i
for x in xs invariant P do ... end for       // elements of a list
match e
case Variant(field: binder, ...) then ...
case 42 then ...           // literal patterns for Int and String
else ...                   // required unless enum cases are exhaustive
end match
assert P                   // proved statically, no runtime cost
check P                    // runtime check; the verifier assumes P afterwards
break
continue
f(...)                     // a call as a statement
```

A name may not be re-declared while it is in scope (no shadowing). A function whose
return type is not `Unit` must return on every path.

## Expressions

- Literals: `42`, `"text"`, `true`, `false`, `[a, b, c]`, `[]`, `{}` (empty map).
  `[]`, `{}` and `Option.None` need an expected type from context.
- Variables, `result` (in `ensures`), `it` (in refinements).
- Calls: `f(a: 1, b: 2)` or `f(x)` for single-parameter functions.
- Construction: `Point(x: 1, y: 2)`, `Shape.Rect(w: 1, h: 2)`, `Shape.Empty`,
  `Option.Some(value: 3)`, `Option.None`.
- Field access `p.x`, index `xs[i]` (lists give elements, strings give the byte as `Int`),
  functional update `p with (x: 3, y: 4)`.
- Unary: `-e`, `not e`.
- Bounded quantifiers: `forall i in a..b: P` and `exists i in a..b: P` over `a <= i < b`.
  They are proved symbolically in specifications and run as loops at runtime.
- Binary: `+ - * / %` (Int), `++` (String or List concatenation), `== !=` (Int, Bool,
  String), `< <= > >=` (Int), `and or implies` (Bool, short-circuit).
- Holes: `?name`. A program with holes does not compile. The compiler reports each
  hole's expected type, the variables in scope and the facts known at that point.

**No precedence.** A binary expression may not contain a different binary operator
without parentheses. `a + b + c` is fine because `+`, `*`, `++`, `and` and `or` may
chain with themselves. `a + b * c`, `a - b - c` and `a < b < c` are errors. Unary
operators and postfix forms bind tighter than any binary operator.

`/` and `%` use Euclidean semantics (the remainder is always non-negative), matching
SMT-LIB. The divisor must be provably non-zero.

## Built-in functions

| Function | Notes |
|---|---|
| `len(x) -> Int where it >= 0` | List, String or Map |
| `push(list: xs, item: x) -> List<T>` | amortised O(1) |
| `slice(of: x, from: a, to: b)` | List or String; requires `0 <= a <= b <= len(x)` |
| `join(parts: List<String>, sep: String) -> String` | |
| `int_to_str(n) -> String` | |
| `str_to_int(s) -> Option<Int>` | |
| `char(code) -> String` | requires `0 <= code < 256` |
| `insert(map: m, key: k, value: v) -> Map<K, V>` | |
| `lookup(map: m, key: k) -> Option<V>` | |
| `has(map: m, key: k) -> Bool` | |
| `keys(m) -> List<K>` | ascending order |
| `hash(s) -> String` | FNV-1a 64-bit, 16 hex digits |
| `panic(msg)` | aborts; never returns |
| `print(s)`, `eprint(s)` | `uses console`; no newline is added |
| `read_file(path) -> Option<String>` | `uses fs` |
| `write_file(path: p, contents: c) -> Bool` | `uses fs` |
| `make_dir(path) -> Bool` | `uses fs`; creates missing parents; true if the directory exists afterwards |
| `list_dir(path) -> Option<List<String>>` | `uses fs`; sorted names, without `.` and `..` |
| `args() -> List<String>` | `uses proc`; excludes the program name |
| `getenv(name) -> Option<String>` | `uses proc` |
| `run(argv: List<String>, input: String) -> Output` | `uses proc`; runs a subprocess |
| `exit(code)` | `uses proc`; never returns |
| `value_or(opt: o, default: d) -> T` | the `Some` value, or `d` |
| `is_some(o) -> Bool`, `is_none(o) -> Bool` | |

## Verification

The verifier performs symbolic execution of each function body and sends proof
obligations to Z3 in SMT-LIB 2.

- `Int` maps to mathematical integers. This is sound because overflow traps at runtime:
  any execution that continues has the mathematical value.
- Every other type maps to an uninterpreted sort `Val`, with uninterpreted functions
  for `len`, string bytes, list elements, struct fields and enum tags. Facts from
  construction are asserted: field values, lengths, and contents (`push` keeps earlier
  elements and appends one; `++` and `slice` relate every element or byte of the result
  to their inputs; literals give each element).
- **Obligations:** index bounds; non-zero divisors; `requires` of callees; built-in
  preconditions; refinements of values flowing into refined positions; `ensures` and
  return refinements at each `return`; `assert`; loop invariants on entry and
  preservation.
- **Assumptions:** branch conditions; `match` tags; refinements of parameters, fields
  read and call results; callee `ensures`; `check`; invariants at loop heads.
- Loops: variables assigned in the body (by `set` or as `inout` arguments) are
  havocked at the loop head, then the invariants and loop condition are assumed.
  Invariants must hold on entry, after each iteration and at every `break` and
  `continue`. After a loop without `break`, the invariants and the negated condition
  hold. A range loop `for i in a..b` is verified as the counted loop it is: invariants
  may mention `i`, which is `a` on entry and `i + 1` after an iteration, and `i = b`
  holds after the loop.
- **Termination** is proved where a measure is given. For `while ... decreases M`, `M`
  must be non-negative when an iteration starts and strictly smaller at its end and at
  every `continue`. For a function with `decreases M`, every directly recursive call
  must have a smaller non-negative `M` (measured on the arguments as passed). `for`
  loops always terminate. Functions without measures get partial-correctness proofs:
  their contracts hold if they return. Mutual recursion is not yet measured.
- Unknown or timed-out queries count as failures. Failed obligations are reported with
  a counterexample when Z3 provides one.

Specification expressions (`requires`, `ensures`, `where`, `invariant`, `decreases`,
`assert`, `check`) are restricted to pure, total-in-the-logic forms: literals,
variables, `it`, `result`, arithmetic, comparisons, boolean operators, quantifiers,
field access, indexing, the built-ins `len`, `value_or`, `is_some` and `is_none`, and
calls to expression functions.

## Effects

`console`, `fs` and `proc`. Built-ins carry the effects listed above. A call is allowed
only if the caller declares every effect the callee uses. `main` may declare any.

## Content addressing

For each function the compiler computes:

- **body hash:** FNV-1a over a canonical rendering of the function;
- **verification key:** the body hash combined with the contract hashes of every
  function called in the body or the specifications, and the hash of all type
  declarations. An expression function's contract hash covers its body, because callers'
  proofs see it.

Verified keys are stored in `<file>.vowcache`. Unchanged functions are not re-verified.
`vowc hash file.vow` prints each function's hashes.

## Runtime model

Every value is one 64-bit word. Integers and booleans are unboxed. Strings, lists,
maps, structs and enums are pointers to immutable heap objects. Memory comes from a bump
allocator and is never freed. That suits batch programs such as compilers; long-running
programs would need a collector.
