# Why Vow's verifier is sound

This is the argument that a program `vowc` accepts behaves as its contracts say. It is
an argument, not a machine-checked proof: each rule the verifier uses is stated with
the reason it is sound, so that a reviewer can check the reasons and a change to the
verifier can be checked against them. Where a rule rests on the checker, the runtime or
the cache, that is said too. [SPEC.md](SPEC.md) describes what the verifier assumes and
proves; this says why that is enough.

## The claim

For a program that `vowc check` accepts, every run of it that reaches a point in the
code satisfies everything proved there:

- no index, slice, `set_at` or `char` argument is out of range and no divisor is zero;
- each callee's `requires` and parameter refinements hold when it is called;
- each `ensures` and return refinement holds when a function returns, and each
  refinement of a variable, field or argument holds whenever a value is stored there;
- each loop invariant holds on entry, at the top of every iteration, at `break` and
  `continue`, and when the loop ends; each `assert` holds where it is written;
- where a function or loop has a `decreases` measure, the measure is non-negative and
  decreases, so the loop or recursion terminates.

This is partial correctness: a run that does not return (it loops forever without a
measure, overflows, fails a `check`, panics or exits) makes no claim from that point
on. Integer overflow traps at runtime, so every run that continues past an arithmetic
operation has its mathematical value, which is what the verifier reasons with.

`vowc build --check-contracts` compiles every one of these claims into a runtime check
(see SPEC.md, Runtime model), so a failed claim in a run of a verified program shows up
as a panic naming it. That is how the claims are tested (see the last section).

## What is trusted

- **Z3** (version 5.3.2, pinned): `unsat` is taken as a proof. Only `unsat` counts;
  `sat`, `unknown`, a timeout or an error are reported as failures.
- **The C compiler and the runtime** (`runtime/vow_rt.h`): they implement the operations
  as the verifier models them. The runtime's own checks (bounds, division, `char`) are
  compiled out of verified programs, so each of them must be an obligation the verifier
  proves; all eight are (`tests/run/divmod.vow` pins the one place where C and SMT-LIB
  could differ, integer division, which both make Euclidean).
- **The front end**: parsing, module resolution and type checking produce the program
  the verifier sees. The verifier relies on the checker for the rules marked
  *(checker)* below.
- **The cache**: a function whose verification key is cached is not verified again
  (see The cache).

## The encoding

Each function is verified on its own, by symbolic execution of its body: every value
gets an SMT term, a *state* maps each variable to its current term and carries the
*path facts* known at that point, and each obligation is a query "path facts and not
goal", which must be unsatisfiable.

**Definitions are consistent.** Facts that hold on every path are asserted globally,
but only about fresh constants (the result of `push`, a literal, a call's result...),
and only facts that some value satisfies whatever else is true: a
fresh constant can always be chosen to satisfy them. Facts that need the operation's
precondition (a slice's length is `to - from`, `char`'s byte is its argument) are path
facts, asserted only after the precondition is proved. Each function's definitions are
checked to be satisfiable before its obligations (an `unsat` there is an internal
error). Quantified content facts (`++`, `push`, `slice`, `set_at`) come after that
check, and are consistent because each describes only the contents of a fresh constant.
The other quantified facts are true of every value: the prelude's (a length is at least
0, a byte is below 256), the float facts (see Floats), the definitions of the structural
equality predicates (a least fixpoint, built one level at a time, see Equality) and the
axioms of specification functions (true of every real value, see below).

A construction (`S(...)`, a variant with fields, `with`) is the exception: its fields
are path facts, stated after its field refinements are proved, not global ones. A global
fact would describe a value that breaks the refinements wherever the construction cannot
run (on a path whose guard is false, or with an argument that only that path rules
out), and the axioms of specification functions hold only for real values, so together
they could prove anything.

**Values are real.** At runtime every value is a finite tree built bottom-up by
constructors and built-ins; nothing can build a value that breaks its type's field
refinements, because every way of building one checks them: constructors and variant
constructors check each field's refinement, `with` checks the refinements of the fields
it sets and of the fields whose refinements name them, and a generic body that builds a
value of a generic type is checked once for every type argument. The verifier relies on
this in three places: reading a field assumes its refinement; a `match` arm assumes the
variant's field refinements; and a specification being proved assumes the field
refinements of the struct values it names (`struct_hyps`), as hypotheses of the goal
rather than facts, because a term on a path that cannot run need not denote a real
value. `with` also assumes the field refinements of the value it copies, which is real.
And the axioms of a recursive specification function, which state its definition and
postconditions, were proved only for real arguments, so they state them only for
arguments that satisfy its preconditions and whose structs satisfy their field
refinements (and the postconditions only where the result's do too). Unguarded, they
would be false of an unreal argument: for a struct that no value can satisfy, a body
that returns -1 proves `ensures result > 0`, and an axiom saying both of every value is
a contradiction.

**Refinements stay true.** A refinement is checked when a value is stored in a refined
position and assumed when it is read back, and in two places without a fresh check:
at a loop head, for the variables the loop assigns, and in the caller after a call,
for the variables passed inout. Both are sound only if a refinement cannot become false
while its variable is unchanged, which holds because a refinement of a `var` or an
inout parameter may name only variables that cannot change: `let`s and parameters that
are not inout *(checker)*. Field refinements may name sibling fields, but struct values
are immutable, and `with` rechecks the refinements of the fields it leaves when they
name ones it changes.

**Every change to a variable is seen.** A variable changes only by `set` or by being
passed inout, and only a plain variable can be passed inout *(parser)*. The verifier
updates its term at each, and the loop havoc (below) collects them from the whole body,
nested blocks and every expression included. An expression whose evaluation is
conditional, the right operand of `and`, `or` and `implies`, may pass a variable inout:
the variable's term afterwards is "the new value if the operand was evaluated, the old
one otherwise". A quantifier's body runs any number of times, and a `while` condition
once before every iteration, so neither may pass anything inout *(checker)*: a condition
has no effect, which the loop rules rely on. No variable is shadowed *(checker)*, so the state's single
map from names to terms is unambiguous.

## Rules

**Calls.** A call proves the callee's `requires` and parameter refinements for the
arguments, then assumes its `ensures` and return refinement, with `old(x)` the value
passed and `x` a fresh term for each inout argument. This is sound because the callee is
verified to establish those on every return, given those on entry. Recursion makes the
argument circular: for runs that return it is an induction on the depth of the call
tree, which is finite in any run that returns (partial correctness). Arguments are
evaluated in written order, in the verifier and in the generated C alike, and a
variable passed inout may not be changed by another argument of the same call, nor
passed inout twice *(checker)*.

**Branches.** An `if` or `match` verifies each branch from its guard (a `match` arm
from its own pattern only, which is sound because a match's patterns are pairwise
distinct *(checker)*); afterwards the
state assumes that one of the branches that did not end (by `return`, `break`,
`continue` or a call that never returns) was taken, keeps each branch's facts under its
guard, and gives each variable the `ite` of its terms across those branches. A `match` on
an enum or option is exhaustive (its tag is one of the variants'), and one on an `Int`
or `String` must have an `else` *(checker)*, so the assumption that some arm ran holds.
Only `panic` and `exit` are taken never to return, and a program cannot declare
functions with those names *(checker)*. A function with a result cannot reach the end
of its body *(checker)*, so its postconditions are checked only at its `return`s.

**Loops.** At the head of a loop every variable the body may assign gets a fresh term
constrained by its refinement (sound by "Refinements stay true") and the invariants
(proved on entry and at every way back to the head: the end of the body and each
`continue`). After the loop the state is the head's, after the condition (which changes nothing,
so this describes every state the loop can leave in: the values at a `break` satisfy the
invariants and refinements, as the head's do); if the body has no `break` (looking
into nested `if` and `match`, but not nested loops, whose breaks are theirs), the
condition is false there too. A range loop's bounds are evaluated once, before it. A `break` proves the invariants. A range loop's variable is
`lo` on entry and one more on each way back, and after a loop without `break` it is
`hi`, or `lo` if that is larger.
A `for x in xs` loop iterates the list as it was when the loop began (the generated code
holds a reference to it), and `x` is an element of that list.

**Specifications are well defined.** The verifier translates indexing, Int division and
calls in a specification as total functions, but a build with `--check-contracts`
evaluates the specification. So wherever a specification is checked (a parameter
refinement or `requires` on entry, given the ones before it; an `ensures` or result
refinement at each return; an invariant, a loop measure, an `assert`, a stored value's
refinement, a field refinement at a construction, a function's measure on entry), it is
also proved defined: each index
in range, each Int divisor not zero and each call's preconditions true, counting only
the parts that run (the right operand of `and`, `or` and `implies` where the left one
lets it run, a quantifier's body for each value in range). A specification assumed
rather than checked was proved defined where it was checked. In a checked build, a
function called while a specification is evaluated does not check its own contracts:
they are proved, and checking them could recurse forever (a postcondition that calls its
own function).

**Specification functions.** A non-recursive function whose body is `let`, `if`,
`match` and `return` is inlined where a specification calls it. A recursive one is an
uninterpreted function defined by its body, as axioms in three layers so that each use
unfolds it at most twice, and each layer is also given the postconditions; all hold only
where the preconditions do and the arguments are real (see "Values are real"). These axioms are consistent if the function terminates on
every input satisfying its preconditions: then its body defines a unique function, which
satisfies its proved postconditions. So a recursive function may be used in a
specification only with a `decreases` measure *(checker)*, which its own verification
proves decreases, and the axioms are left out of the queries of its own recursive group,
whose proofs establish that termination and so cannot assume it. Groups are the
strongly connected components of the call graph, counting calls in contracts and in the
refinements a proof assumes: those of its parameters, result and annotated locals
(written directly or through a type alias), and the field refinements of the structs and
enums it handles (a node for each type, with edges to the functions its field
refinements call). So specifications can only rely on other groups, which form an
acyclic order. (Which functions are recursive is decided by calls alone.)

**Termination.** A loop measure is proved non-negative at the top of each iteration
and smaller at the end of the body and at each `continue`. A call within the caller's
recursive group proves the callee's measure, on the arguments as passed, smaller than
the caller's, which is non-negative; since every call in a cycle of the group decreases
the same well-founded quantity, no run recurses forever. A structural measure is a
value's rank, which the verifier relates only as a value's real height is related:
a variant's fields and a list's elements are smaller than the value, and a struct's
fields no larger. These facts are stated for the parts the code reads, so every rank
fact is true of heights, and a rank proved smaller is a height that is smaller.

**Generics.** A generic function is verified once, with each type parameter an
uninterpreted sort about which nothing is known, and its equality an undefined
predicate. A proof that holds for every interpretation of the sort holds for every type
argument, so the contract holds for every instance. Because a type argument may be
`Float`, or a type holding one, `x == x` is not assumed for type parameters: a NaN is not
equal to itself.

**Floats.** A `Float` is known only by whether it is a NaN and, if not, by its position
in the order of doubles. Comparisons are defined through these, and the only other facts
are those true of IEEE doubles under any rounding: the exact values of small whole
literals, the order of literals, and monotonicity or exactness of negation, `abs_float`,
`floor`, `ceil`, `sqrt` and `int_to_float`. Arithmetic results are otherwise unknown.
The generated C performs each operation as written, without fused multiply-add, so the
double computed is the one the program describes.

**Equality.** `==` is equality of the logic on `Int` and `Bool` and equality of terms on
`String`, and on other types either equality of terms or a structural predicate defined
one level at a time. Every fact used is true of the runtime's equality: equal terms
denote the same value, and equal values have equal parts. Nothing is asserted that would
make two terms unequal unless their contents differ.

## The cache

A function is not verified again when its verification key is in the program's cache
file. The key is a hash of everything the proof depended on: the verifier version; the
function's own text; every type declaration and every specification function's whole
text (refinements and specifications may use them, however indirectly); the contracts
of the functions it calls, measures included; and the members of its recursive group,
since a call into the group carries a measure obligation that a call outside it does
not. The verifier version changes whenever the SMT the verifier emits does
(`tests/fingerprint.sh` enforces this), so a result proved by an older verifier, which
may have had a bug since fixed, is proved again. `tests/cache/` edits programs between
two checks with the cache on.

## Known limits

- Termination is proved only where measures are given; elsewhere contracts hold for
  runs that return.
- `--check-contracts` does not check the contracts of functions called while a
  specification is evaluated. It checks a structural measure by the value's height,
  which is one rank the verifier's facts allow, not the only one.
- Integer arithmetic is proved for mathematical integers; a run that overflows traps.
- The argument above is informal. The rules it lists are the verifier's; a rule the
  verifier applies but that is missing here is a gap in this document, and should be
  treated as a possible soundness bug until it is argued.

## How it is tested

- **Must-fail tests** (`tests/fail/`): programs that must be rejected, each with the
  message expected. Every soundness bug found becomes one.
- **Checked builds**: `make test` builds every test program, example and benchmark,
  and the compiler (verifying and compiling itself), with `--check-contracts` under the
  debug runtime. A verified program cannot fail a check, so a failure is a bug.
- **Mutation fuzzing** (`tests/fuzz.py`, `make fuzz` and `make fuzz-compiler`): small
  changes to verified programs, modules, benchmarks and the compiler's own source; a
  mutant that verifies and then fails a runtime check is a soundness bug. With the check
  that loop invariants are preserved deliberately removed, it finds the hole within a
  few hundred mutants.
- **Generated programs** (`tests/genfuzz.py`, `make fuzz-gen`): random well-typed
  programs full of contracts (refined aliases, structs and enums, recursive and mutually
  recursive specification functions, inout parameters with `old`, loops with invariants
  and measures, quantifiers, floats). Each contract the verifier rejects is removed until
  the program verifies; then `main` calls every function over a grid of inputs with
  contract checks on, and a failed check, or a checked build that hangs or crashes where
  the plain build runs, is a soundness bug (`tests/reduce.py` shrinks one to a small
  program). Run against v0.9.2, it finds four of the six bug classes the review found.
- **Differential testing**: the frozen Rust bootstrap verifier and the self-hosted one
  must emit identical SMT for every program in the v0.3 language.

## Soundness bugs found

Each was a rule that assumed something again where the check that justified it did not
reach. Each now has a test.

- v0.9.1: a refinement of a `var` or inout parameter that named another changing
  variable was assumed again at loop heads and after calls, after a change to the
  other variable had made it false (`tests/fail/refine_var_dep.vow`,
  `refine_inout_dep.vow`).
- After v0.9.1: a variable passed inout on the right of `and`, `or` or `implies` kept
  its old term (`tests/fail/inout_and.vow`, `tests/run/short_circuit.vow`), and one
  passed inout in a quantifier body was not modelled at all (`inout_forall.vow`).
- After v0.9.1: a function's cached proof was reused after an edit elsewhere made it part
  of a recursive group, skipping the measure obligations of its calls into the group
  (`tests/cache/group`).

The independent review after v0.9.2 found six more:

- A `Float` literal's exponent was read by its length, not its value, so `1.0e-0000001`
  (0.1) was taken for 10^-1000000 and ordered below `0.05`; that proved false facts, an
  out-of-range index among them (`tests/fail/float_exp_zeros.vow`).
- A variable passed inout in a `while` condition kept its old term at the loop head; a
  `break` left with the state after the condition; and a measure taken after the
  condition could be raised again by it, so a loop proved to terminate did not
  (`inout_while.vow`).
- A construction's fields were global facts, so outside the path that built a value
  (or for an argument its refinement ruled out) the axioms of a specification function,
  true only of real values, proved anything (`unreal_struct.vow`).
- A call in the refinement of a type alias was not an edge of the call graph, so two
  functions proved their postconditions from each other's (`alias_cycle.vow`).
- Specifications were not proved defined: an index out of range, a zero divisor or a
  call breaking its callee's precondition made a checked build fail
  (`spec_index.vow`, `spec_div.vow`, `spec_pre.vow`).
- In a checked build, a postcondition that called its own function recursed forever
  (`tests/run/contract_self.vow`).

The program generator found one more after v0.9.3:

- The axioms of a specification function over a struct were not limited to real
  arguments. When no value could satisfy the struct's field refinements, its
  postconditions were proved vacuously, the axioms contradicted each other, and any
  specification naming the function (a loop measure) proved anything
  (`empty_struct_axiom.vow`).
