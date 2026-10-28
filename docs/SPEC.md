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
- Floats: digits, `.`, digits, then optionally `e`, a sign and digits: `1.5`, `0.0`,
  `2.5e-3`. Both sides of the `.` need a digit (`1.` and `.5` are not literals), so
  `0..n` is still a range.
- Strings: `"..."` with escapes `\n \t \r \\ \" \0`.
- Characters: `'a'`, `'\n'`, `'\''` are `Int` literals holding one byte.
- Holes: `?name`.
- Newlines are insignificant everywhere.

Keywords:
`fn end struct enum type let var set return if then elif else while for in do match case
invariant requires ensures uses assert check break continue and or not implies true false
with import it result where inout forall exists decreases try`

`as` and `private` are not keywords: they have their meaning only in an import and
before a declaration, and remain ordinary names elsewhere.

A keyword may be used as a **label**: a struct or variant field name, a named-argument
label, a field access (`d.type`), a `with` field or a pattern field. Keywords may not
name variables, parameters or binders.

## Program structure

A program is a sequence of declarations, which may appear in any order. Import paths
are resolved relative to the importing file, and each file is loaded once. Paths
beginning with `std/` name standard library modules built into the compiler, such as
`std/text.vow`. There are three forms of import:

- `import "util.vow"` merges util.vow into the importing file's namespace. The main
  file and every file reached from it by merging imports share one global namespace,
  in which every public name must be unique.
- `import "geo.vow" as geo` makes geo.vow a module: its public declarations are named
  `geo.name`. Functions are called as `geo.dist(...)`, structs constructed as
  `geo.Point(x: 1, y: 2)`, variants as `geo.Shape.Box(...)` and `geo.Shape.Empty`, and
  types written `geo.Point` and `List<geo.Point>`. Patterns use the bare variant name,
  as always. Only geo.vow's own declarations are reachable this way, not those of the
  files it imports. A module name may not also name a variable, parameter or field,
  and two imports in a file may not use the same one.
- `import "std/text.vow" (split, trim)` brings just those public declarations of the
  file into the importing file's namespace, unqualified. A file's own declaration of
  the same name is an error.

A declaration marked `private` (`private fn`, `private struct`, `private enum`,
`private type`) is visible only in its own file, however the file is imported; two
files may each have a private declaration of the same name. Every other declaration
is public. Diagnostics name a module's declarations by their qualified names
(`json.Json`), using the file's name for the module.

```
import "util.vow"
import "std/json.vow" as json

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

### Packages

A package is a git repository of Vow files. A project lists the packages it uses in a
file named `vow.pkg`, one per line: a name, a git URL, and the commit to use, in full
(40 hex digits). Blank lines and lines starting with `#` are ignored.

```
# name  git URL                               commit
shapes  https://github.com/someone/vow-shapes  0123456789abcdef0123456789abcdef01234567
```

A file imports a package's files as `"@name/path.vow"`, usually as a module:
`import "@shapes/shapes.vow" as shapes`. The `vow.pkg` that applies to a file is the
nearest one in the file's directory or a directory above it.

- `vowc add <name> <git URL> [ref]` pins the package to the commit that `ref` (a
  branch, a tag, or `HEAD` when omitted) names now, in the nearest `vow.pkg` (creating
  `./vow.pkg` if there is none), and fetches it.
- `vowc fetch` fetches every package listed in the nearest `vow.pkg`, and the packages
  those list in their own `vow.pkg`, into `.vow/pkg/` beside it. Each package is
  stored by URL and commit, so two versions of a package can be used side by side.
  A file in a package resolves `"@name"` through its package's own `vow.pkg`.

Fetching runs nothing but `git`. A package's code is checked and verified with the
program that uses it, like any other file: nothing is trusted because it was
published. Commit `vow.pkg`; `.vow/` can be fetched again.

## Types

| Type | Meaning |
|---|---|
| `Int` | 64-bit signed integer; overflow traps at runtime |
| `Float` | 64-bit IEEE double; arithmetic never traps (dividing by zero gives an infinity or NaN) |
| `Bool` | `true` / `false` |
| `String` | immutable byte string |
| `Unit` | the empty value; the return type of functions without `->` |
| `List<T>` | immutable list |
| `Map<K, V>` | immutable ordered map (a balanced tree); `K` is `Int` or `String` |
| `HashMap<K, V>` | the same values as `Map`, stored in a hash table; `K` is `Int` or `String` |
| `Option<T>` | built-in enum: `Some(value: T)` and `None` |
| `Result<T, E>` | predeclared generic enum: `Ok(value: T)` and `Err(error: E)` (see Generics) |
| `Output` | built-in struct: `code: Int, out: String` (result of `run`) |
| `T where P` | refinement: values of `T` satisfying predicate `P` over `it` |
| user `struct` / `enum` | nominal; enums may be recursive; may be generic |

All values are immutable. Local `var` bindings may be reassigned with `set`, and a
function may take `inout` parameters, which gives mutable-value semantics without
aliasing:

```
fn emit(g: inout Gen, line: String) do
  set g = g with (lines: push(list: g.lines, item: line))
end fn emit

emit(g: inout gen, line: "x")      // 'inout' is written at the call site too
```

An `inout` argument must be a `var` (or an `inout` parameter of the caller). The same
var may not be passed `inout` twice in one call, and no other argument of that call may
modify it (for example by passing it `inout` to a nested call). Semantically the callee receives the
value and the caller's var receives the callee's final value when the call returns.
In `ensures`, an `inout` parameter names its final value; in `requires`, its initial
value. In `ensures` and in loop invariants, `old(x)` names the value the `inout`
parameter `x` had when the function was entered:

```
fn push_all(out: inout List<Int>, xs: List<Int>)
  ensures len(out) == (len(old(out)) + len(xs))
  ensures forall i in 0..len(old(out)): out[i] == old(out)[i]
do
  ...
end fn push_all
```

`old` takes only the name of an `inout` parameter, and is reserved: no function may be
called `old`. The parameter's refinement must hold whenever it is assigned and is assumed by
the caller afterwards.

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

More generally, a pure function returning a value, with no `inout` parameters and no
type parameters, whose body uses only `let`, `if`/`elif`/`else`, `match`, `assert` and
`return` over specification expressions (and calls only such functions) is a
**specification function**, and may be called in specifications. A recursive one
(alone or with others) needs a `decreases` measure:

```
fn size(t: Tree) -> Int
  ensures result >= 0
  decreases t
do
  match t
  case Leaf then
    return 0
  case Node(left: l, value: v, right: r) then
    return (size(l) + 1) + size(r)
  end match
end fn size

fn in_order(t: Tree, out: inout List<Int>)
  ensures len(out) == (len(old(out)) + size(t))
```

A non-recursive specification function's body is translated into a term. A recursive
one is a function of the logic defined by its body, under its preconditions, along with
its postconditions; each use is unfolded at most twice, so a claim that needs deeper
unfolding needs intermediate `assert`s. These definitions are left out of the proofs of
the function's own recursive group: they are consistent only because the function
terminates, which those proofs establish. A specification function is transparent to the
verifier only when a specification uses it; elsewhere its calls are known by their
contracts alone, like any other function's.

`struct` and `enum` are shown above. Fields and variants are comma-separated, with an
optional trailing comma. `type Name = T` declares an alias.

## Generics

Functions, structs and enums may have type parameters:

```
struct Pair<A, B>
  first: A,
  second: B,
end struct Pair

enum Tree<T>
  Leaf,
  Node(left: Tree<T>, value: T, right: Tree<T>),
end enum Tree

fn reversed<T>(xs: List<T>) -> List<T>
  ensures len(result) == len(xs)
do
  ...
end fn reversed
```

- A generic type is written with its arguments: `Pair<Int, String>`, `Tree<T>`.
- Type arguments of calls and constructors are never written; they are inferred from
  the expected type (an annotation, the parameter being filled, the function's return
  type) and from the arguments. `reversed([1, 2])` is `reversed` at `Int`;
  `Pair(first: 1, second: "a")` is a `Pair<Int, String>`; `Tree.Leaf` and
  `Result.Err(error: e)` need an expected type. When a parameter cannot be inferred,
  the error asks for an annotation.
- Inside the declaration, a type parameter is a type about which nothing is known:
  values of type `T` can be stored, passed, returned, compared with `==` and put in
  lists, options, maps and other generic types, but not indexed or given fields. A
  use of `==` on `T` is checked again for each type argument, so a generic function
  that compares its `T` values cannot be used with a map type. A type
  parameter may be a map key; each use checks that its key type is `Int` or `String`.
- Type parameters may not have the name of a type, and type arguments may not be
  refined (wrap the element in a struct with a refined field instead).
- Generic functions cannot be used in specifications.
- Each generic declaration is type-checked and verified once, with its type
  parameters as unknown types (in the logic, values of the uninterpreted sort `Val`).
  A proof that holds for every interpretation of `Val` holds for every type argument,
  so the contracts of a generic function hold for every use. Each use with concrete
  types is compiled to its own copy of the code (monomorphisation).
- A generic function that calls itself must do so with the same type arguments.

`Result<T, E>` is declared in every program (unless the program declares its own type
named `Result`):

```
enum Result<T, E>
  Ok(value: T),
  Err(error: E),
end enum Result
```

A variant whose fields all have type `Unit` may be written without them, so a
function that can only succeed or fail returns `Result<Unit, E>` and writes
`return Result.Ok`.

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
let x = try e              // also var x = try e, set x = try e, and try f(...)
break
continue
f(...)                     // a call as a statement
```

A name may not be re-declared while it is in scope (no shadowing). A function whose
return type is not `Unit` must return on every path.

**`try`** propagates failure. It may only be used in a function that returns a
`Result` or an `Option`, and only as the whole value of `let`, `var` or `set`, or as a
call statement. In a function returning `Result<U, E>`, `let x = try e` requires `e`
to be a `Result<T, E>` with the same error type: if `e` is `Err(error: x)` the function
returns `Result.Err(error: x)`; otherwise `x` is bound to the `Ok` value. In a function
returning `Option<U>`, `e` must be an `Option<T>`, and `None` is returned. `try` is
shorthand for the `match` it expands to, and the verifier sees that `match`.

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
- Binary: `+ - * / %` (Int), `+ - * /` (Float), `++` (String or List concatenation),
  `== !=` (any two values of the same type without maps; see Equality), `< <= > >=`
  (Int or Float). `Int` and `Float` never mix: convert with `int_to_float`.
  `and or implies` (Bool, short-circuit).

**Equality.** `==` is structural: strings are equal when their bytes are; lists when
they have the same length and equal elements; structs when every field is equal;
enums and options when the variants are the same and their fields are equal. All
`Unit` values are equal. Values whose type is or contains a `Map` or `HashMap` cannot
be compared; compare lookups instead. Floats compare as IEEE doubles: a NaN is not equal
to anything, itself included, so a struct, list or option holding a NaN is not equal to
itself either.
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
| `len(x) -> Int where it >= 0` | List, String, Map or HashMap |
| `push(list: xs, item: x) -> List<T>` | amortised O(1) |
| `set_at(list: xs, at: i, value: v) -> List<T>` | `xs` with element `i` replaced; requires `0 <= i < len(xs)`; in place when `xs` is unshared |
| `slice(of: x, from: a, to: b)` | List or String; requires `0 <= a <= b <= len(x)` |
| `join(parts: List<String>, sep: String) -> String` | |
| `int_to_str(n) -> String` | |
| `str_to_int(s) -> Option<Int>` | |
| `char(code) -> String` | requires `0 <= code < 256` |
| `insert(map: m, key: k, value: v) -> M` | `M` is `Map<K, V>` or `HashMap<K, V>`, as `m` |
| `lookup(map: m, key: k) -> Option<V>` | |
| `has(map: m, key: k) -> Bool` | |
| `keys(m) -> List<K>` | ascending order, for both map types |
| `hash(s) -> String` | FNV-1a 64-bit, 16 hex digits |
| `panic(msg)` | prints `vow: panic at <file>:<line>: msg` to stderr, exits with status 101; never returns |
| `print(s)`, `eprint(s)` | `uses console`; no newline is added |
| `int_to_float(n) -> Float` | exact for magnitudes up to 2^53 |
| `float_to_int(x) -> Option<Int>` | rounded toward zero; `None` for NaN or outside the `Int` range |
| `float_to_fixed(x: x, digits: d) -> String` | `x` with exactly `d` decimals (0 to 40), as C's `printf("%.*f")` writes it |
| `float_to_str(x) -> String` | the shortest text that reads back as `x`, as Python prints it (`0.1`, `300.0`, `1e+16`, `nan`, `inf`) |
| `str_to_float(s) -> Option<Float>` | the whole string must be a number |
| `floor(x)`, `ceil(x)`, `round(x)`, `sqrt(x)`, `abs_float(x)` | `Float -> Float`; `round` rounds halves away from zero |
| `is_nan(x) -> Bool` | |
| `read_line() -> Option<String>` | `uses console`; the next line of standard input without its `\n`, or `None` at the end |
| `read_bytes(n) -> String` | `uses console`; up to `n` bytes of standard input (fewer at the end) |
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
  read and call results; callee `ensures`; `check`; invariants at loop heads. A
  specification being proved (an `assert`, `ensures`, loop invariant, callee
  `requires` or refinement) also assumes the field refinements of the struct values it
  names (variables, parameters, `it`, `result`, `old(x)`, their struct fields, and the
  elements of lists among them): on any path that runs, those are real values of their
  types. These are hypotheses of the goal, not facts, since a value built on a path
  that cannot happen need not be real. For a list, the hypothesis covers the elements
  in range, so `forall k in 0..len(xs): xs[k].count == len(xs[k].values)` is proved.
- Loops: variables assigned in the body (by `set` or as `inout` arguments) are
  havocked at the loop head, then the invariants and loop condition are assumed.
  Invariants must hold on entry, after each iteration and at every `break` and
  `continue`. After a loop without `break`, the invariants and the negated condition
  hold. A range loop `for i in a..b` is verified as the counted loop it is: invariants
  may mention `i`, which is `a` on entry and `i + 1` after an iteration, and `i = b`
  holds after the loop.
- **Termination** is proved where a measure is given. For `while ... decreases M`, `M`
  must be non-negative when an iteration starts and strictly smaller at its end and at
  every `continue`. Functions that call each other, directly or through others, form a
  recursive group (calls in contracts count). When one member of a group has
  `decreases M`, every member must have a measure, and every call within the group
  must make the callee's measure, on the arguments as passed, smaller than the
  caller's `M`, which must be non-negative. `for` loops always terminate. Functions
  without measures get partial-correctness proofs: their contracts hold if they return.
- A measure is an `Int`, or a value of a list, struct, enum or option type, which must
  get structurally smaller: a variant's fields and a list's elements are smaller than
  the value, and a struct's fields are no larger than the struct (so recursion through
  a struct must also pass through a variant or a list). These facts are stated where
  the code reads the part: in a `match` arm, at `xs[i]` and `for x in xs`, and at a
  struct field. `decreases t` on a tree proves `size(l) + size(r)` terminates.
- `vowc check --totality` reports which functions are proved to terminate: those
  whose `while` loops and recursion are measured and which call only such functions.
  Built-in functions are taken to terminate.
- Unknown or timed-out queries count as failures. Failed obligations are reported with
  a counterexample when Z3 provides one.

Specification expressions (`requires`, `ensures`, `where`, `invariant`, `decreases`,
`assert`, `check`) are restricted to pure, total-in-the-logic forms: literals,
variables, `it`, `result`, arithmetic, comparisons, boolean operators, quantifiers,
field access, indexing, the built-ins `len`, `value_or`, `is_some` and `is_none`,
calls to specification functions, and (in `ensures` and loop invariants) `old(x)`.

Generic functions are verified once for all type arguments (see Generics); calls to
them are checked against their contracts with the type arguments of the call.

**Floats.** A `Float` is a value the verifier knows only two things about: whether it is
a NaN, and, if not, its place in the order of doubles (a real number that is the
double's value, with the infinities beyond every finite double). Comparisons are
defined through these, so facts about order (`x < y` and `y < z` give `x < z`; a
comparison with a NaN is false) come for free. A literal is not a NaN; a literal that
is a whole number below 10^15 has its exact value; and two literals are ordered as their
decimal values are (strictly when both have at most 15 significant digits). Negation,
`abs_float`, `floor`, `ceil`, `sqrt` and `int_to_float` of an integer up to 2^53 have
their exact or monotone facts. Nothing else about arithmetic is assumed, since rounding
makes most algebraic laws false (`(0.1 + 0.2) == 0.3` is false): state what a proof
needs with `check` (tested at runtime), a refinement or a contract.

`==` on `Int` and `Bool` is equality of the logic, and on `String` equality of terms.
On other types `a == b` is `a = b` or `eq_T(a, b)`, where `eq_T` is a predicate
defined by an axiom that unfolds it one level: the fields, the tag and the variant's
fields, or the length and every element, compared the same way. Equal values can
therefore be proved equal however they were built, and equal values have equal parts.
For a type parameter, `eq_T` is left undefined, so a generic function proves only what
holds for every type's equality. Since a type parameter may stand for `Float`, and a type
that can hold a float is not equal to itself when it holds a NaN, `x == x` is not
assumed for such types.

## Effects

`console`, `fs` and `proc`. Built-ins carry the effects listed above. A call is allowed
only if the caller declares every effect the callee uses. `main` may declare any.

## Content addressing

For each function the compiler computes:

- **body hash:** FNV-1a over a canonical rendering of the function;
- **verification key:** the verifier version, the body hash, the contract hashes of
  every function called in the body or the specifications, and the hash of all type
  declarations. The verifier version changes whenever the SMT the verifier emits does,
  so a result is reused only by the verifier that proved it. A specification function's contract hash covers its body, because
  callers' proofs see it, and the hash of all type declarations also covers every
  specification function's body (so a change to one invalidates proofs that use it
  indirectly).

Verified keys are stored in `<file>.vowcache`. Unchanged functions are not re-verified.
`vowc hash file.vow` prints each function's hashes.

## Testing

`vowc test file.vow` checks and verifies the program, then runs each test: a function in
that file (not in the files it imports) whose name starts with `test_`, which must take
no parameters and return nothing. Each test runs in its own process, so a failed
`check` or a panic fails only that test; the report shows the output of each failing
test, including its panic message, and the command exits with status 1 if any failed.
A file of tests needs no `main`. `--json` prints one object per test
(`{"kind":"test","name":...,"status":"pass"|"fail","output":...}`).

## Formatting

`vowc fmt file.vow` rewrites a file in the canonical layout, changing only whitespace:
line breaks, comments and tokens are kept, and the result is written only if it lexes
to the same tokens and has the same comments. `--check` reports instead of writing.

- Indentation is two spaces per block. `end`, `else`, `elif` and `case` line up with the
  line that opened their block (so `case` lines up with `match`). `requires`, `ensures`,
  `invariant`, `decreases` and `uses` on their own lines are indented one step under
  their header. A line that continues a statement (inside an open bracket, or after a
  line ending in a binary operator) is indented one step past the statement, or two
  when the statement opens a block (`fn`, `if`, `elif`, `while`, `for`, `match`,
  `case`).
- One space separates tokens, except: none just inside `()`, `[]`, `{}`; none before
  `,`, `:` or `.`, or after `.`; none around `..`; none after a unary `-`; none before
  the `(` or `[` of a call or index; none inside type arguments (`Map<String, Int>`). A
  `<` opens type arguments right after a capitalised name written without a space, or
  after the name in a `fn`, `struct`, `enum` or `type` declaration; otherwise it compares.
- A comment on its own line is indented like the code after it (or, before `end`, like
  the block's body). A trailing comment keeps its distance from the code.
- No trailing whitespace; at most one blank line in a row; exactly one blank line
  between top-level declarations; none at the start or end of the file; a final newline.

## Runtime model

Every value is one 64-bit word. Integers and booleans are unboxed. Strings, lists,
maps, structs and enums are pointers to immutable heap objects, each with a reference
count. An object is freed as soon as nothing refers to it (cycles cannot occur, because
values are immutable). When an object has a single reference, `with`, `push`, `set_at`
and `insert` update it in place instead of copying; nothing else can observe the
difference, so value semantics are preserved. In `set x = op(x, ...)`, the old value of
`x` is moved into `op`, which is what makes such updates in place. In
`set s = s with (f: op(s.f, ...))`, where `s.f` is read only once, the field is moved
out of `s` as well, so a list or map held in a struct is updated in place too.

A `Map` is a balanced tree: a shared map copies only the path to the changed key, so
snapshots are cheap. A `HashMap` is an open-addressing hash table: lookups and inserts
are faster, but an insert into a shared table copies the whole table. The two types
hold the same values and are verified identically; only their cost differs.

The generated C carries `#line` directives, so C compiler messages and debuggers refer
to `.vow` lines (`--no-line-directives` leaves them out). A runtime failure prints
`vow: panic at <file>:<line>: <message>` to stderr and exits with status 101: integer
overflow, `panic(msg)`, a failed `check`, and, in programs built with `--no-verify`,
out-of-bounds indexes, slices and `set_at`, division by zero and `char` codes out of
range (verified programs cannot fail these, so the checks are compiled out).
