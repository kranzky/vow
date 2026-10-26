# Vow for Humans

This is not a tutorial. It's for people who work alongside language models that write
code, and who want to know what changes when those models write Vow instead of
Python, TypeScript or C.

## The problem Vow is aimed at

Models now write a lot of code, and they write it fast. The bottleneck has moved to
review. Generated code is usually plausible and often right, but "plausible" is
exactly what makes review hard: the bugs that survive are the ones that look fine.
An off-by-one in a loop bound, a function that quietly writes a file, a missing case
in a branch. Catching them means reading every line of the body, carefully, every
time.

Vow moves as much of that checking as possible from your reading to the compiler, and
concentrates what is left on a small surface you can review well.

## What you get

### Review signatures, not bodies

In Vow, what a function promises is written in its signature, and the compiler
proves the body keeps that promise:

```
fn clamp(x: Int, lo: Int, hi: Int) -> Int
  requires lo <= hi
  ensures (lo <= result) and (result <= hi)
```

If this compiles, then for every input satisfying `lo <= hi` the result is between
`lo` and `hi`. That's a proof over all inputs, not a test of a few. Your review
question changes from "is this body correct?" to "is this the right promise?", which is
shorter, more stable, and closer to what you actually care about.

Callers are checked against the promise, never the body. If a model rewrites the body
of `clamp`, nothing that calls it needs re-review as long as the signature is unchanged.

### Whole classes of bugs are gone

In a program that compiles:

- no list or string index is out of bounds;
- no division is by zero;
- no `slice` has an invalid range;
- every value in a refined type (`Int where it >= 0`, a struct field
  `pos: Int where it < len(toks)`) satisfies its refinement, everywhere;
- every loop invariant the author wrote actually holds.

None of this is runtime checking. These are proofs, and in a fully verified program
the compiled code contains no bounds checks at all. Vow's own compiler, about 6,000
lines written by a model, is verified this way and uses no runtime-checked escape
hatches.

### Properties you actually care about

Contracts can say more than "no crash". From the examples:

```
fn escape(s: String) -> String
  ensures tag_free(result)                  // no '<' or '>' anywhere in the output

fn search(xs: List<Int>, target: Int) -> Int
  requires is_sorted(xs)
  ensures (result >= 0) implies (xs[result] == target)
  ensures (result == -1) implies (forall i in 0..len(xs): xs[i] != target)
  ...
```

The first is a security property, an escaper that provably cannot emit markup. The
second is the full correctness of binary search. Both are proved for every input, and if
someone breaks the code, the compiler shows the input that breaks it. Change the
escaper so `<` slips through and the counterexample is `c = 60`, which is `'<'`.

Loops and recursion can also carry a termination measure (`decreases hi - lo`), which
proves they finish. The self-hosted compiler's lexer is proved to terminate on every
input this way.

### Effects are an audit trail

A Vow function can only print, touch files or run processes if its signature says so:
`uses console`, `uses fs`, `uses proc`. This is enforced transitively, so a function
without `uses` is pure, and so is everything it calls.

This makes some review questions mechanical. "Can this parser write to disk?" Look at
its signature. "Which parts of this codebase can run subprocesses?"
`grep "uses .*proc"`. When a model adds file access deep inside a helper, the
signature change ripples up through every caller, and that shows in the diff.

### Disagreements come with evidence

When the compiler cannot prove something, it says exactly what and often shows a
counterexample:

```
error: cannot prove loop invariant is preserved: total <= i
    counterexample: i = 1, n = 1, total = 2
```

That changes the conversation with a model, or with a colleague, from "this looks
wrong to me" to "here are the inputs where it fails". Models are good at acting on
precise feedback like this; it's the same signal the compiler gives them.

### Holes are hand-off points

A model can leave a typed hole, `?decision`, where it needs a human call. The compiler
reports what the hole must be, what's in scope and everything known at that point:

```
hole: ?upper needs a value of type Int
    in scope: x: Int, lo: Int, hi: Int
    known facts: lo <= hi; not (x < lo); x > hi
```

A program with holes never compiles, so a pending decision can't ship by accident.

### Code that is harder to misread

Some of Vow's rules exist for models, but they help human readers just as much:

- No operator precedence: `a + (b * c)` is spelled out.
- Named arguments: `transfer(from: a, to: b, amount: n)` rather than three
  positional values.
- No shadowing, no implicit conversions, no null, no exceptions.
- Every block says what it closes: `end fn transfer`, `end while`.

### Changes show what they affect

`vowc hash` prints a hash of each function's body and of its contract. In a code
review, an unchanged contract hash means callers are unaffected, whatever happened to
the body. The verifier uses the same information to skip re-verifying functions
whose dependencies didn't change.

## A workflow that uses this

1. **People own the contracts.** Write, or at least approve, the signatures: types,
   refinements, `requires`, `ensures`, `uses`. These are the specification.
2. **Models write the bodies,** iterating against the verifier until everything is
   proved.
3. **CI runs `vowc check`.** A merge needs every proof to go through. `--json` gives
   machine-readable diagnostics for bots and dashboards.
4. **Reviewers read contract diffs closely and body diffs lightly,** and look hard at
   any `check` statement (a runtime-checked assumption) and any new `uses`.

## What Vow does not guarantee

Being clear about the limits matters more than the guarantees.

- **A proof is only as good as its specification.** If a contract says too little,
  the proof says too little. `ensures result >= 0` is easy to satisfy with the wrong
  answer. Reviewing contracts is the job that remains, and it's real work.
- **Termination is proved only where someone wrote a measure.** Without `decreases`,
  a proved contract holds *if* the function returns; a loop that never ends is not
  caught. Mutual recursion can't be measured yet.
- **Integer overflow is not proved absent.** It's detected at runtime and stops the
  program, which is safe but not graceful.
- **Specifications can't yet say everything.** Quantifiers range over integer
  intervals, and functions used in specs must be single expressions without recursion.
  Some properties, such as "the output is a permutation of the input", are still hard
  to state.
- **Effects are coarse.** `uses fs` means any file, not a particular directory.
- **There is a trusted base:** the Vow verifier, the Z3 solver, the C compiler and
  the runtime. The verifier is new. Two soundness bugs (cases where something was
  assumed instead of proved) were found and fixed while it was being built, and there
  may be others. V1 calls for a written soundness argument and differential fuzzing.
- **There are escape hatches.** `check` turns an unproved claim into a runtime test,
  and `--no-verify` skips proofs entirely. Both are visible: `check` in the source,
  `--no-verify` in your build configuration.
- **Memory is reference counted**, so it is reclaimed promptly, but the reference
  counting is in the generated C and the runtime, outside what the verifier proves. It
  is tested heavily (including a debug runtime that catches any use after free), not
  proved.

## Status

Vow v0 is an experiment: a language designed by a model for models, small enough to
compile and verify itself. It is not production-ready. See [`ROADMAP.md`](../ROADMAP.md)
for what a V1 needs.
