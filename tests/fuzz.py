#!/usr/bin/env python3
"""Soundness fuzzing by mutation.

Takes the verified programs in tests/run and examples, makes one small change to one
of them (an integer literal, a comparison, + and -, and or or, true or false), and runs
the result with `vowc run --check-contracts`, which verifies it first. Most mutants
fail to verify. One that verifies has had every contract proved, so if the run then
fails a contract, refinement, invariant, measure, assert, bounds or division check, the
verifier proved something false: a soundness bug. Such mutants are saved with the
failure.

    tests/fuzz.py [--count N] [--seed S] [--jobs J] [--out DIR] [--vowc PATH] [--specs F]

--specs F (default 0.5) is the share of mutants changed in a contract line (requires,
ensures, invariant, decreases, assert, or a where refinement), where a change that
still verifies is most likely to expose a wrong proof.

Integer overflow, a failed `check` and the program's own panics are allowed in verified
code, so they are not findings; nor are mutants that run too long.
"""

import argparse
import concurrent.futures
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CORPUS_DIRS = ["tests/run", "examples"]
# Copied too, because corpus programs import them.
SUPPORT_DIRS = ["tests/modules", "examples/money"]

# Panics that a verified program can never reach: reaching one is a soundness bug.
BUG_MESSAGES = [
    "precondition failed",
    "postcondition failed",
    "refinement of",
    "loop invariant failed",
    "loop measure",
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
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return None, "", ""


def is_bug(stderr):
    for line in stderr.splitlines():
        if line.startswith("vow: panic at") and any(m in line for m in BUG_MESSAGES):
            return line
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--count", type=int, default=200)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--out", default="fuzz-findings")
    ap.add_argument("--vowc", default=os.path.join(ROOT, "build", "vowc"))
    ap.add_argument("--timeout", type=int, default=60)
    ap.add_argument("--specs", type=float, default=0.5)
    args = ap.parse_args()
    vowc = os.path.abspath(args.vowc)
    rng = random.Random(args.seed)

    work = tempfile.mkdtemp(prefix="vow-fuzz-")
    try:
        for d in CORPUS_DIRS + SUPPORT_DIRS:
            shutil.copytree(os.path.join(ROOT, d), os.path.join(work, d), dirs_exist_ok=True,
                            ignore=shutil.ignore_patterns("*.vowcache"))
        # The corpus: programs that pass as they are, with checks on.
        corpus = []
        for d in CORPUS_DIRS:
            for name in sorted(os.listdir(os.path.join(work, d))):
                if not name.endswith(".vow"):
                    continue
                rel = os.path.join(d, name)
                code, _, err = run([vowc, "run", rel, "--check-contracts", "--no-cache"], work, args.timeout)
                if code == 0:
                    with open(os.path.join(work, rel)) as f:
                        src = f.read()
                    corpus.append((rel, src, sites(src)))
                else:
                    print(f"skipped {rel}: does not run cleanly with --check-contracts", file=sys.stderr)
        total = sum(len(s) for _, _, s in corpus)
        print(f"{len(corpus)} programs, {total} mutation sites; trying {args.count} mutants (seed {args.seed})")

        jobs = []
        with_specs = [c for c in corpus if any(s[3] for s in c[2])]
        for i in range(args.count):
            if with_specs and rng.random() < args.specs:
                rel, src, ss = rng.choice(with_specs)
                ss = [s for s in ss if s[3]]
            else:
                rel, src, ss = rng.choice(corpus)
            if not ss:
                continue
            a, b, r, _ = rng.choice(ss)
            jobs.append((i, rel, src[:a] + r + src[b:], src, a))

        def attempt(job):
            i, rel, mutant, src, at = job
            d, name = os.path.split(rel)
            path = os.path.join(d, f"fz{i}_{name}")
            with open(os.path.join(work, path), "w") as f:
                f.write(mutant)
            code, out, err = run([vowc, "run", path, "--check-contracts", "--no-cache"], work, args.timeout)
            os.remove(os.path.join(work, path))
            line_no = src.count("\n", 0, at) + 1
            if code is None:
                return ("timeout", rel, line_no, None, mutant)
            if code == 0:
                return ("verified", rel, line_no, None, mutant)
            bug = is_bug(err)
            if bug:
                return ("BUG", rel, line_no, bug, mutant)
            if "vow: panic" in err:
                return ("panic", rel, line_no, None, mutant)
            return ("rejected", rel, line_no, None, mutant)

        counts = {}
        findings = 0
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as ex:
            for kind, rel, line_no, bug, mutant in ex.map(attempt, jobs):
                counts[kind] = counts.get(kind, 0) + 1
                if kind == "BUG":
                    findings += 1
                    os.makedirs(args.out, exist_ok=True)
                    base = os.path.join(args.out, f"{findings}_{os.path.basename(rel)}")
                    with open(base, "w") as f:
                        f.write(f"// fuzz finding: mutant of {rel} at line {line_no}\n// {bug}\n" + mutant)
                    print(f"BUG {rel}:{line_no}: {bug} (saved {base})")
        print(", ".join(f"{v} {k}" for k, v in sorted(counts.items())))
        return 1 if findings else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
