# How Vow compares

Vow is a small, young language with one unusual goal: to be easy for a *language
model* to write correctly, and easy for a *person* to trust afterwards. This page
compares it with languages you probably know, credits the languages its ideas come
from, and is plain about what is actually new.

The short version: almost every ingredient of Vow exists somewhere else. What is
unusual is the combination, and the reason for it.

## At a glance

| | Vow | Python | TypeScript | Go | Rust | C | Dafny |
|---|---|---|---|---|---|---|---|
| Index out of bounds | **proved impossible** at compile time | runtime `IndexError` | returns `undefined` | runtime panic | runtime panic | undefined behaviour | proved impossible |
| Null | none; `Option` | `None` anywhere | tracked with `strictNullChecks` | `nil` | none; `Option` | `NULL` | only in nullable types |
| Contracts (`requires`/`ensures`) | **proved by Z3**, always on | runtime `assert` only | none | none | none built in | runtime `assert` only | proved |
| Loop invariants | proved | none | none | none | none | none | proved |
| Termination | proved where a measure is given | no | no | no | no | no | proved by default |
| Side effects in signatures | **`uses console, fs, proc`**, checked | no | no | no | no | no | heap frames; no I/O effects |
| Operator precedence | **none**: mixing needs parentheses | yes | yes | yes (5 levels) | yes | yes (15 levels) | yes |
| Argument passing | **named**, except one-parameter calls | positional or keyword | positional | positional | positional | positional | mostly positional |
| Mutation | values are immutable; `var` and `inout`, no aliasing | shared references | shared references | shared references | ownership and borrowing | raw pointers | heap with `modifies` clauses |
| Memory management | reference counting, in-place updates when unshared | reference counting and GC | GC | GC | ownership | manual | GC |
| Generics | no (v0) | duck typing | yes | yes | yes | no | yes |
| Compiles to | C | bytecode | JavaScript | native | native | native | C#, Java, JS, Go, Python, ... |
| Maturity | experimental | very mature | very mature | mature | mature | very mature | mature, used in industry |

## What is unusual about Vow

### It is designed for machine authors

Languages are usually designed for people. Vow's rules were chosen for the
mistakes a language model makes:

- **No operator precedence.** `a + b * c` is an error; write `a + (b * c)`. Nothing has
  to be remembered or counted, and nothing can be silently misread.
- **Every block names what it closes:** `end fn search`, `end while`, `end match`. A
  missing delimiter is reported where it happens, not hundreds of lines later.
- **Arguments are named** (`transfer(from: a, to: b, amount: n)`), so they cannot be
  passed in the wrong order.
- **No shadowing, no implicit conversions, no null, no exceptions.** Each rules out a
  class of plausible-looking bug.
- **Errors are written for the next attempt:** every failed proof names the exact
  obligation and, when it can, the inputs that break it.

The documentation is part of the design. [Vow for LLMs](VOW_FOR_LLMS.md) is written for a
model to read once before writing Vow. As a test, another model (OpenAI Codex) wrote a
545-line static site generator, the tool that builds this website, from the
documentation alone. It reports no parse, type or verification errors along the way
([its report](../tools/vowsite/REPORT.md)). That is one data point, not a study.

### Proofs are on by default, without a proof language

In Vow, contracts, refinement types, loop invariants, index bounds and division by
zero are checked by the Z3 solver every time you compile. There is no separate proof
mode, and no lemmas, ghost code or tactics to learn. If it compiles, the contracts
hold.

Dafny does this too, and does more: it has lemmas, ghost code, generics, termination
by default, and years of industrial use. Vow trades that power for a smaller
language that a model can use correctly on the first attempt. Some properties that
Dafny can prove, Vow cannot even state yet.

### Effects are a short list in the signature

A function is pure unless it declares `uses console`, `uses fs` or `uses proc`, and
callers must declare everything their callees use. So "can this parser touch the
file system?" is answered by reading one line.

Koka, F* and Haskell track effects far more expressively, with effect types,
handlers or monads. Vow's version is deliberately blunt: three named capabilities, no
type-level machinery, nothing to learn beyond the word `uses`.

### Holes show what the verifier knows

Write `?name` where an expression should go, and the compiler reports the expected
type, the variables in scope and every fact known at that point:

```
hole: ?upper needs a value of type Int
    in scope: x: Int, lo: Int, hi: Int
    known facts: lo <= hi; not (x < lo); x > hi
```

Agda, Idris and Lean show goals and hypotheses at holes when you write proofs. Vow
brings the same idea to everyday imperative code, where the "hypotheses" are the
branch conditions and invariants on the path to the hole.

### Proofs are content-addressed

A function's proof is cached under a hash of its body plus the *contracts* (not the
bodies) of everything it calls. Rewrite a function's body without changing its
signature, and none of its callers are re-verified. Unison identifies *code* by
content hash; Vow does the same for *proofs*.

### The compiler verifies itself

Vow's compiler is about 6,000 lines of Vow. It verifies itself: every index, slice
and refinement in it is proved, its text utilities have functional specifications,
and its lexer is proved to terminate on every input. It uses no runtime-checked escape
hatches. It also reaches a fixpoint: compiled by itself, it produces byte-identical C.

This proves memory safety and the stated contracts, not that the compiler is correct
end to end. CompCert, a C compiler proved correct in Coq, is in a different league.

## Performance

We timed three programs from the
[Computer Language Benchmarks Game](https://benchmarksgame-team.pages.debian.net/benchmarksgame/),
written as the same straightforward, single-threaded algorithm in every language (the
Game's fastest entries use SIMD and multiple threads, which would measure tuning rather
than languages). Every output was checked against C's. Times are the best of three runs
on an Apple M1 Pro; memory is the peak footprint.

| Benchmark | Vow | C | Rust | Go | JavaScript | Python |
|---|---|---|---|---|---|---|
| binary-trees, depth 18 (allocation) | **0.92 s**, 49 MB | 1.14 s, 17 MB | 1.19 s, 17 MB | 1.58 s, 42 MB | 0.59 s, 427 MB | 5.60 s, 38 MB |
| fannkuch-redux, n = 10 (array updates) | 0.39 s, 1 MB | 0.18 s, 1 MB | 0.19 s, 1 MB | 0.18 s, 2 MB | 0.24 s, 12 MB | 4.55 s, 6 MB |
| k-nucleotide, 500k bases (hash maps) | 1.11 s, 21 MB | 0.06 s, 18 MB | 0.11 s, 15 MB | 0.11 s, 31 MB | 0.35 s, 69 MB | 0.56 s, 23 MB |

Full results, toolchain versions and the programs are in
[the benchmark results](../bench/RESULTS.md); `bench/run.py` reproduces them.

What the numbers say:

- **Memory use is in line with C, Rust and Go.** Vow counts references: an object is
  freed as soon as nothing refers to it.
- **Updates happen in place when nothing else can see them.** Vow values are
  immutable, but when a list or struct has only one reference, `set_at`, `push`,
  `with` and `insert` change it in place, and nobody can observe the difference.
  fannkuch-redux, which swaps elements millions of times, runs about twice as long as C.
- **Allocation is fast.** binary-trees, which allocates and frees millions of small
  objects, runs faster than C's `malloc`/`free`, thanks to a size-class allocator.
- **Maps are persistent trees, not hash tables.** k-nucleotide is 18 times slower than
  C and twice as slow as Python. A hash map is on the [roadmap](../ROADMAP.md).
- **Proofs remove bounds checks**, so verified indexing costs nothing at runtime.
- **Floating-point benchmarks can't run yet:** n-body, spectral-norm and mandelbrot
  need `Float`, which Vow doesn't have.

The in-place updates follow the approach of Koka's Perceus, Lean 4 and Swift's
copy-on-write. Before them (Vow 0.1, which never freed memory), fannkuch-redux took
4.34 s and 19.7 GB.

## Where Vow's ideas come from

| Idea | Where it comes from |
|---|---|
| Contracts proved by an SMT solver | Dafny, SPARK Ada, Why3 |
| Refinement types (`Int where it >= 0`) | Liquid Haskell, F* |
| Effects in function signatures | Koka, F*, Haskell |
| No operator precedence, mixing needs parentheses | Pony |
| Labelled block ends (`end if`, `end name`) | Ada, Fortran |
| Mutable value semantics, `inout` | Swift, Hylo |
| Typed holes | Agda, Idris, Haskell, Lean |
| Content addressing | Unison |
| Compiling to C, bootstrapping from a C seed | Nim, Zig and many others |

## When not to use Vow

Today, for most things. Vow v0 is an experiment:

- no floating-point numbers, generics, modules, closures or concurrency;
- macOS and Linux only;
- no package ecosystem;
- termination and overflow are not proved unless you ask for them (`decreases`) or
  check them at runtime (overflow traps);
- programs built around large maps are slow (see Performance above).

Use Python or TypeScript for breadth, Go or Rust for production systems, and Dafny or
SPARK when you need machine-checked proofs today. Use Vow to explore what a language
designed for model authors could be, or for small, self-contained tools where a proof
is worth more than a library. See the [roadmap](../ROADMAP.md) for what V1 needs.
