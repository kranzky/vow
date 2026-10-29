#!/usr/bin/env python3
"""Shrinks a fuzzing finding.

Removes statements, blocks and declarations from a Vow program (and unwraps blocks)
while it still verifies and then fails a runtime check whose message contains TEXT.

    tests/reduce.py FINDING.vow OUT.vow [--text TEXT] [--jobs J] [--vowc PATH]

TEXT defaults to the message in the finding's header (the line `// vow: panic at
gen.vow:N: MESSAGE` that tests/genfuzz.py writes). OUT.vow is rewritten after every
step, so it can be read while the reduction runs.
"""

import argparse
import concurrent.futures
import os
import re
import shutil
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fuzz import ROOT, run  # noqa: E402

# Lines that continue the statement or declaration above them at the same indentation.
CONT = ("end ", "elif ", "else", "case ", "do", ")", "requires", "ensures", "decreases",
        "invariant", "where", ",")


def indent(s):
    return len(s) - len(s.lstrip(" "))


def span(lines, i):
    """Line i and the lines that belong to it: deeper ones, and continuations (elif,
    else, case, end, ...) at its own indentation."""
    n = indent(lines[i])
    j = i + 1
    while j < len(lines):
        s = lines[j]
        if s.strip() == "" or indent(s) > n:
            j += 1
            continue
        if indent(s) == n and s.strip().startswith(CONT) and not lines[i].strip().startswith(("case ", "else")):
            j += 1
            if s.strip().startswith("end "):
                break
            continue
        break
    return i, j


def candidates(lines):
    out = []
    for i, s in enumerate(lines):
        t = s.strip()
        if not t or t.startswith(("//", "end ", "do", ")")):
            continue
        a, b = span(lines, i)
        out.append((a, b, False))
        if b - a > 1 and not t.startswith(("fn ", "struct ", "enum ", "type ")):
            out.append((a, b, True))  # keep the body, drop the block around it
    out.sort(key=lambda c: c[0] - c[1])  # biggest first
    return out


def apply(lines, c):
    a, b, unwrap = c
    if not unwrap:
        return lines[:a] + lines[b:]
    body = [l for l in lines[a + 1:b] if indent(l) > indent(lines[a])]
    shift = indent(body[0]) - indent(lines[a]) if body else 0
    return lines[:a] + [l[shift:] if l.startswith(" " * shift) else l for l in body] + lines[b:]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("finding")
    ap.add_argument("out")
    ap.add_argument("--text", default=None)
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--vowc", default=os.path.join(ROOT, "build", "vowc"))
    ap.add_argument("--timeout", type=int, default=60)
    args = ap.parse_args()
    vowc = os.path.abspath(args.vowc)
    lines = open(args.finding).read().splitlines()
    text = args.text
    if text is None:
        m = next((re.search(r"// vow: panic at [^:]+:\d+: (.*)$", l) for l in lines[:3]
                  if re.search(r"// vow: panic at", l)), None)
        if not m:
            print("no panic message in the finding's header: give --text", file=sys.stderr)
            return 2
        text = m.group(1)
    base = tempfile.mkdtemp(prefix="vow-reduce-")

    def holds(k, ls):
        d = os.path.join(base, f"t{k}")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "r.vow"), "w") as f:
            f.write("\n".join(ls) + "\n")
        code, _, err = run([vowc, "run", "r.vow", "--check-contracts", "--no-cache"], d, args.timeout)
        return code is not None and "error:" not in err and "vow: panic" in err and text in err

    def save():
        with open(args.out, "w") as f:
            f.write("\n".join(lines) + "\n")

    try:
        if not holds("start", lines):
            print(f"the finding does not fail with '{text}'", file=sys.stderr)
            return 2
        changed = True
        while changed:
            changed = False
            cands = candidates(lines)
            k = 0
            while k < len(cands):
                trials = []
                for c in cands[k:k + args.jobs]:
                    try:
                        trials.append(apply(lines, c))
                    except IndexError:
                        trials.append(None)
                with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as ex:
                    ok = list(ex.map(lambda t: t[1] is not None and holds(t[0], t[1]), enumerate(trials)))
                hit = next((i for i, r in enumerate(ok) if r), None)
                if hit is None:
                    k += args.jobs
                    continue
                # Take the first (biggest) removal that keeps the failure, and start over.
                lines = trials[hit]
                changed = True
                save()
                print(f"{len(lines)} lines", flush=True)
                cands = candidates(lines)
                k = 0
        save()
        print(f"done: {len(lines)} lines in {args.out}")
        return 0
    finally:
        shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
