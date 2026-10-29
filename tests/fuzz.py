#!/usr/bin/env python3
"""Soundness fuzzing by mutation.

Makes one small change (an integer literal, a comparison, + and -, and or or, true or
false) to a verified Vow file, and exercises the result with contract checks on
(--check-contracts), which verifies it first. Most mutants fail to verify. One that
verifies has had every contract proved, so if it then fails a contract, refinement,
invariant, measure, assert, bounds or division check, the verifier proved something
false: a soundness bug. Such mutants are saved with the failure.

    tests/fuzz.py [--target T] [--count N] [--seed S] [--jobs J] [--out DIR]
                  [--vowc PATH] [--specs F]

--target programs (the default) mutates the test programs, the examples, the modules
they import and the benchmarks, and runs the program. --target compiler mutates the
compiler's own source, builds the mutant with checks, and has it verify and compile a
few test programs: the compiler is the largest verified program there is.

--specs F (default 0.5) is the share of mutants changed in a contract line (requires,
ensures, invariant, decreases, assert, or a where refinement), where a change that
still verifies is most likely to expose a wrong proof.

Integer overflow, a failed `check` and the program's own panics are allowed in verified
code, so they are not findings; nor are mutants that run too long.
"""

import argparse
import concurrent.futures
import glob
import os
import random
import re
import shutil
import signal
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Copied into each mutant's directory.
DIRS = ["tests/run", "tests/modules", "examples", "bench"]

# Modules, with a program that imports them.
MODULES = {"tests/modules/geo.vow": "tests/run/modules.vow",
           "tests/modules/util.vow": "tests/run/modules.vow",
           "examples/money/money.vow": "examples/prices.vow"}
# Benchmarks, with small arguments (knucleotide needs an input file, so it is left out).
BENCH = {"bench/binarytrees/binarytrees.vow": ["8"], "bench/fannkuch/fannkuch.vow": ["7"],
         "bench/nbody/nbody.vow": ["500"], "bench/spectralnorm/spectralnorm.vow": ["50"],
         "bench/mandelbrot/mandelbrot.vow": ["100"]}
# What a mutant compiler verifies and compiles.
COMPILER_INPUTS = ["tests/run/features.vow", "tests/run/modules.vow", "examples/calc.vow",
                   "examples/bsearch.vow", "tests/run/termination.vow"]
# Generated from other files: mutating them would only mutate a string.
GENERATED = {"compiler/runtime_h.vow", "compiler/std_text.vow", "compiler/std_json.vow"}

# Panics that a verified program can never reach: reaching one is a soundness bug.
BUG_MESSAGES = [
    "precondition failed",
    "postcondition failed",
    "refinement of",
    "loop invariant failed",
    "loop measure",
    "did not decrease",
    "assertion failed",
    "out of bounds",
    "division by zero",
    "char code",
]

SWAPS = {"<": "<=", "<=": "<", ">": ">=", ">=": ">", "==": "!=", "!=": "==",
         "+": "-", "-": "+", "and": "or", "or": "and", "true": "false", "false": "true"}
# An operator token: comparisons, spaced binary + and -, and, or, true, false. '<' and
# '>' next to a type name or in '->' are not comparisons.
OP = re.compile(r"(?<![-<>=!])(<=|>=|==|!=|<(?![A-Z=])|(?<![-A-Za-z])>(?!=))(?![=>])"
                r"|(?<= )[+-](?= )|\b(and|or|true|false)\b")
INT = re.compile(r"(?<![\w.])\d+(?![\w.])")
SPEC_LINE = re.compile(r"^\s*(requires|ensures|invariant|decreases|assert)\b|\bwhere\b")


def sites(src):
    """(start, end, replacement, in_spec) for every mutation, outside comments and
    strings."""
    out = []
    pos = 0
    for line in src.splitlines(keepends=True):
        code = line
        spec = bool(SPEC_LINE.search(line.split("//")[0]))
        cut = code.find("//")
        if cut >= 0:
            code = code[:cut]
        # Blank out string and character literals.
        masked = re.sub(r'"(\\.|[^"\\])*"|\'(\\.|[^\'\\])*\'', lambda m: " " * len(m.group(0)), code)
        for m in OP.finditer(masked):
            tok = m.group(0)
            if tok in SWAPS:
                out.append((pos + m.start(), pos + m.end(), SWAPS[tok], spec))
        for m in INT.finditer(masked):
            n = int(m.group(0))
            for r in {n + 1, max(n - 1, 0)} - {n}:
                out.append((pos + m.start(), pos + m.end(), str(r), spec))
        pos += len(line)
    return out


def run(cmd, cwd, timeout):
    # In its own process group, so that a timeout also kills the program `vowc run`
    # started: killing only vowc would leave a looping mutant running.
    p = subprocess.Popen(cmd, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                         errors="replace", stdin=subprocess.DEVNULL, start_new_session=True)
    try:
        out, err = p.communicate(timeout=timeout)
        return p.returncode, out, err
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        p.communicate()
        return None, "", ""


def is_bug(stderr):
    for line in stderr.splitlines():
        if line.startswith("vow: panic at") and any(m in line for m in BUG_MESSAGES):
            return line
    return None


def program_steps(vowc, prog, args):
    return [[vowc, "run", prog, "--check-contracts", "--no-cache", "--"] + args]


def compiler_steps(vowc):
    steps = [[vowc, "build", "compiler/vowc.vow", "-o", "fz_vowc", "--check-contracts"]]
    for f in COMPILER_INPUTS:
        steps.append(["./fz_vowc", "check", f, "--no-cache"])
        steps.append(["./fz_vowc", "emit-c", f, "--no-verify"])
    return steps


def exercise(steps, cwd, timeout):
    """'verified' if every step succeeds; 'BUG' (with the panic) if a step fails a
    check; 'rejected' if the first step (verification) fails; otherwise 'panic' or
    'timeout'."""
    for k, cmd in enumerate(steps):
        code, _, err = run(cmd, cwd, timeout)
        if code is None:
            return "timeout", None
        bug = is_bug(err)
        if bug:
            return "BUG", bug
        if code != 0:
            if "vow: panic" in err:
                return "panic", None
            # A mutant compiler may reject a program it should accept: that is the
            # mutation's doing, not a soundness bug.
            return ("rejected" if k == 0 else "other"), None
    return "verified", None


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--target", choices=["programs", "compiler"], default="programs")
    ap.add_argument("--count", type=int, default=200)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--out", default="fuzz-findings")
    ap.add_argument("--vowc", default=os.path.join(ROOT, "build", "vowc"))
    ap.add_argument("--timeout", type=int, default=0)
    ap.add_argument("--specs", type=float, default=0.5)
    args = ap.parse_args()
    vowc = os.path.abspath(args.vowc)
    rng = random.Random(args.seed)
    timeout = args.timeout or (900 if args.target == "compiler" else 60)

    base = tempfile.mkdtemp(prefix="vow-fuzz-")
    try:
        # A clean copy of everything a mutant may need; each mutant gets its own copy.
        pristine = os.path.join(base, "pristine")
        dirs = DIRS + (["compiler"] if args.target == "compiler" else [])
        for d in dirs:
            # The compiler keeps its verification cache, so a mutant re-verifies only
            # what the mutation changed.
            ignore = None if d == "compiler" else shutil.ignore_patterns("*.vowcache")
            if d == "bench":
                # Only the Vow programs: the rest is other languages and build output.
                ignore = lambda where, names: [n for n in names
                                               if not n.endswith(".vow") and not os.path.isdir(os.path.join(where, n))]
            shutil.copytree(os.path.join(ROOT, d), os.path.join(pristine, d), ignore=ignore)

        # Targets: (file to mutate, steps that exercise it), kept if the unmutated file
        # passes them.
        targets = []
        if args.target == "compiler":
            for f in sorted(glob.glob(os.path.join(ROOT, "compiler", "*.vow"))):
                rel = os.path.relpath(f, ROOT)
                if rel not in GENERATED:
                    targets.append((rel, compiler_steps(vowc)))
            print("checking that the unmutated compiler passes...", file=sys.stderr)
            kind, _ = exercise(compiler_steps(vowc), pristine, timeout)
            if kind != "verified":
                print("the unmutated compiler does not pass its steps", file=sys.stderr)
                return 2
            for f in ("fz_vowc",):
                p = os.path.join(pristine, f)
                if os.path.exists(p):
                    os.remove(p)
        else:
            for d in ["tests/run", "examples"]:
                for f in sorted(glob.glob(os.path.join(pristine, d, "*.vow"))):
                    rel = os.path.relpath(f, pristine)
                    targets.append((rel, program_steps(vowc, rel, [])))
            for m, prog in MODULES.items():
                targets.append((m, program_steps(vowc, prog, [])))
            for b, bargs in BENCH.items():
                targets.append((b, program_steps(vowc, b, bargs)))
            kept = []
            for rel, steps in targets:
                kind, _ = exercise(steps, pristine, timeout)
                if kind == "verified":
                    kept.append((rel, steps))
                else:
                    print(f"skipped {rel}: does not pass as it is ({kind})", file=sys.stderr)
            targets = kept

        corpus = []
        for rel, steps in targets:
            with open(os.path.join(pristine, rel)) as f:
                src = f.read()
            corpus.append((rel, steps, src, sites(src)))
        total = sum(len(c[3]) for c in corpus)
        print(f"{len(corpus)} files, {total} mutation sites; trying {args.count} mutants (seed {args.seed})")

        jobs = []
        with_specs = [c for c in corpus if any(s[3] for s in c[3])]
        for i in range(args.count):
            if with_specs and rng.random() < args.specs:
                rel, steps, src, ss = rng.choice(with_specs)
                ss = [s for s in ss if s[3]]
            else:
                rel, steps, src, ss = rng.choice(corpus)
            if not ss:
                continue
            a, b, r, _ = rng.choice(ss)
            jobs.append((i, rel, steps, src[:a] + r + src[b:], src, a))

        def attempt(job):
            i, rel, steps, mutant, src, at = job
            work = os.path.join(base, f"m{i}")
            shutil.copytree(pristine, work, symlinks=True)
            with open(os.path.join(work, rel), "w") as f:
                f.write(mutant)
            kind, bug = exercise(steps, work, timeout)
            shutil.rmtree(work, ignore_errors=True)
            return kind, rel, src.count("\n", 0, at) + 1, bug, mutant

        counts = {}
        findings = 0
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as ex:
            for kind, rel, line_no, bug, mutant in ex.map(attempt, jobs):
                counts[kind] = counts.get(kind, 0) + 1
                if kind == "BUG":
                    findings += 1
                    os.makedirs(args.out, exist_ok=True)
                    saved = os.path.join(args.out, f"{findings}_{os.path.basename(rel)}")
                    with open(saved, "w") as f:
                        f.write(f"// fuzz finding: mutant of {rel} at line {line_no}\n// {bug}\n" + mutant)
                    print(f"BUG {rel}:{line_no}: {bug} (saved {saved})", flush=True)
        print(", ".join(f"{v} {k}" for k, v in sorted(counts.items())))
        return 1 if findings else 0
    finally:
        shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
