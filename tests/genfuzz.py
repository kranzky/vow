#!/usr/bin/env python3
"""Soundness fuzzing with generated programs.

Writes random, well-typed Vow programs full of contracts: refined type aliases (some
calling expression functions), structs with refined fields, enums, a recursive tree
type, recursive specification functions, inout parameters with `old`, loops with
invariants and measures, quantifiers, asserts and float literals. Most contracts are
random, so many are false or unprovable. Each program is checked, every contract the
verifier rejects is removed, and the check is repeated until the program verifies.
What remains is a program whose every contract the verifier claims to have proved. It
is then run with contract checks on (--check-contracts), and main calls each function
on several inputs. A contract that fails at runtime was proved false: a soundness bug.
Such programs are saved with the failure.

    tests/genfuzz.py [--count N] [--seed S] [--jobs J] [--out DIR] [--vowc PATH]
                     [--keep DIR]

--keep DIR also saves every program that verified, to read what the generator makes.
--show SEED prints the program for one seed. tests/reduce.py shrinks a finding.

Integer overflow and a program's own panics are allowed in verified code, so they are
not findings; nor are programs that run too long.
"""

import argparse
import concurrent.futures
import os
import random
import re
import shutil
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fuzz import ROOT, run, is_bug  # noqa: E402

INT, BOOL, LIST, TREE, OPT = "Int", "Bool", "List<Int>", "Tree", "Option<Int>"

# ---------------------------------------------------------------------------
# Expressions: tuples, rendered with every binary operation in parentheses.
#   ("lit", n) ("bool", b) ("var", name) ("bin", op, a, b) ("neg", a) ("not", a)
#   ("call", f, [(label, e)]) ("idx", a, i) ("field", a, name) ("len", a)
#   ("quant", "forall"|"exists", var, lo, hi, body) ("old", name) ("raw", text)


def render(e, top=True):
    k = e[0]
    if k == "lit":
        return str(e[1]) if e[1] >= 0 else "-" + str(-e[1])
    if k == "bool":
        return "true" if e[1] else "false"
    if k in ("var", "raw"):
        return e[1]
    if k == "bin":
        s = render(e[2], False) + " " + e[1] + " " + render(e[3], False)
        return s if top else "(" + s + ")"
    if k == "neg":
        return "-" + render(e[1], False)
    if k == "not":
        s = "not " + render(e[1], False)
        return s if top else "(" + s + ")"
    if k == "call":
        return e[1] + "(" + ", ".join((lab + ": " if lab else "") + render(a) for lab, a in e[2]) + ")"
    if k == "idx":
        return render(e[1], False) + "[" + render(e[2]) + "]"
    if k == "field":
        return render(e[1], False) + "." + e[2]
    if k == "len":
        return "len(" + render(e[1]) + ")"
    if k == "quant":
        s = f"{e[1]} {e[2]} in {render(e[3], False)}..{render(e[4], False)}: {render(e[5], False)}"
        return s if top else "(" + s + ")"
    if k == "old":
        return "old(" + e[1] + ")"
    raise ValueError(k)


def norm(text):
    return re.sub(r"[\s()]", "", text)


def B(op, a, b):
    return ("bin", op, a, b)


# ---------------------------------------------------------------------------
# Output: lines, each optionally a removable contract.


class Spec:
    def __init__(self, sid, scope, kind, text, extra=None):
        self.sid, self.scope, self.kind, self.text = sid, scope, kind, text
        self.extra = extra  # a parameter, field or loop the contract belongs to
        self.line = 0
        self.alt = None  # the line written instead when it is removed (None: nothing)


class Out:
    def __init__(self):
        self.lines = []  # (indent, text, Spec or None)
        self.specs = []

    def emit(self, ind, text, spec=None):
        self.lines.append((ind, text, spec))

    def spec(self, ind, prefix, scope, kind, e, extra=None):
        text = render(e)
        s = Spec(len(self.specs), scope, kind, text, extra)
        self.specs.append(s)
        self.emit(ind, prefix + text, s)
        return s

    def choice(self, ind, scope, kind, line, alt):
        """A line the verifier may reject (a recursive call whose argument may not
        decrease the measure), replaced by alt if it does."""
        s = Spec(len(self.specs), scope, kind, line)
        s.alt = alt
        self.specs.append(s)
        self.emit(ind, line, s)
        return s

    def text(self, off):
        out = []
        for ind, t, s in self.lines:
            if s is not None and s.sid in off:
                if s.alt is not None:
                    out.append("  " * ind + s.alt)
                continue
            if s is not None:
                s.line = len(out) + 1
            out.append("  " * ind + t)
        return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------
# Program generator.


class Fn:
    def __init__(self, name, params, ret, kind):
        self.name = name
        self.params = params  # [(name, type, refinement over 'it' or None, inout)]
        self.ret = ret
        self.kind = kind  # "expr", "rec", "plain"


class Gen:
    def __init__(self, rng):
        self.r = rng
        self.o = Out()
        self.n = 0
        self.aliases = {}  # name -> refinement over it
        self.structs = {}  # name -> [(field, type, refinement)]
        self.enum = None  # (name, [(variant, [fields])])
        self.tree = False
        self.fns = {}

    def fresh(self, p):
        self.n += 1
        return f"{p}{self.n}"

    def chance(self, p):
        return self.r.random() < p

    def small(self, lo=-3, hi=12):
        return self.r.randint(lo, hi)

    # -- types -------------------------------------------------------------

    def int_type(self):
        """An Int-like type to declare: Int or an alias."""
        if self.aliases and self.chance(0.3):
            return self.r.choice(sorted(self.aliases))
        return INT

    def is_int(self, t):
        return t == INT or t in self.aliases

    def elem_alias(self):
        """An alias for a list's or option's elements (List<Al3>), or None."""
        if self.aliases and self.chance(0.5):
            return self.r.choice(sorted(self.aliases))
        return None

    def elems_ok(self, alias, lst, on):
        """The condition that every element of the list lst satisfies alias's refinement
        (for a guard), or None if there is nothing to check."""
        conds = [s for s in on if s.scope == alias and s.kind == "alias"]
        if not alias or not conds:
            return None
        k = "kg"
        el = ("raw", "(" + render(lst) + ")[" + k + "]")
        body = " and ".join("(" + render(subst(c.text, {"it": el})) + ")" for c in conds)
        return ("raw", f"forall {k} in 0..len({render(lst)}): ({body})")

    # -- predicates over a value ('it' or a variable) ------------------------

    def pred(self, x, siblings=()):
        c = self.small(-2, 6)
        choices = [
            lambda: B(">=", x, ("lit", c)),
            lambda: B(">", x, ("lit", c)),
            lambda: B("!=", x, ("lit", c)),
            lambda: B("and", B(">=", x, ("lit", c)), B("<=", x, ("lit", c + self.small(0, 20)))),
            lambda: B("==", B("%", x, ("lit", self.r.choice([2, 3, -2]))), ("lit", self.r.randint(0, 1))),
            lambda: B("<", x, ("lit", c + 30)),
        ]
        bool_fns = [f for f in self.fns.values() if f.kind == "expr" and f.ret == BOOL and len(f.params) == 1]
        if bool_fns:
            choices.append(lambda: ("call", self.r.choice(bool_fns).name,
                                    [(self.r.choice(bool_fns).params[0][0], x)]))
        if siblings:
            choices.append(lambda: B(self.r.choice([">=", ">", "<="]), x, ("var", self.r.choice(siblings))))
        int_specs = [f for f in self.fns.values() if f.kind == "spec" and all(self.is_int(t) for _, t, _, _ in f.params)]
        if int_specs:
            def spec_pred():
                f = self.r.choice(int_specs)
                args = [(f.params[0][0], x)] + [(q, ("lit", self.small())) for q, _, _, _ in f.params[1:]]
                return B(self.r.choice([">=", "==", "!=", "<"]), ("call", f.name, args), ("lit", self.small(-2, 4)))
            choices.append(spec_pred)
            choices.append(spec_pred)
        e = self.r.choice(choices)()
        # A call's label must be the callee's own parameter.
        if e[0] == "call":
            f = self.fns[e[1]]
            e = ("call", f.name, [(f.params[0][0], x)])
        return e

    # -- declarations ----------------------------------------------------------

    def expr_fns(self):
        for _ in range(self.r.randint(0, 2)):
            name = self.fresh("ok")
            p = self.fresh("x")
            f = Fn(name, [(p, INT, None, False)], BOOL, "expr")
            body = self.pred(("var", p))
            self.o.emit(0, f"fn {name}({p}: Int) -> Bool do")
            self.o.emit(1, "return " + render(body))
            self.o.emit(0, f"end fn {name}")
            self.o.emit(0, "")
            self.fns[name] = f
        if self.chance(0.5):
            name = self.fresh("mix")
            a, b = self.fresh("a"), self.fresh("b")
            f = Fn(name, [(a, INT, None, False), (b, INT, None, False)], INT, "expr")
            body = self.r.choice([
                B("+", ("var", a), B("*", ("var", b), ("lit", self.small(-2, 3)))),
                B("-", ("var", a), ("var", b)),
                B("/", ("var", a), ("lit", self.r.choice([2, 3, -2, 5]))),
                B("%", B("+", ("var", a), ("var", b)), ("lit", self.r.choice([2, 4, -3]))),
            ])
            self.o.emit(0, f"fn {name}({a}: Int, {b}: Int) -> Int do")
            self.o.emit(1, "return " + render(body))
            self.o.emit(0, f"end fn {name}")
            self.o.emit(0, "")
            self.fns[name] = f

    def spec_group(self, structs=False):
        """Recursive specification functions with no precondition, so they can be called
        anywhere: a group of one to three (mutually recursive when more than one), over
        Int, or over a struct (recursion that never runs, as in tests/fail/unreal_struct).
        Their postconditions may call any member, the function itself included."""
        group = []
        snames = sorted(self.structs) if structs else []
        # Either a chain that counts its first argument down (mutually recursive when
        # there are several), or functions whose only recursion never runs.
        mode = "dead" if snames or self.chance(0.3) else "count"
        for _ in range(self.r.randint(1, 3)):
            name = self.fresh("sf")
            if snames and self.chance(0.6):
                st = self.r.choice(snames)
                f = Fn(name, [(self.fresh("s"), st, None, False)], INT, "spec")
            else:
                # Counting down needs a plain Int; other parameters may be aliases.
                params = [(self.fresh("x"), INT if mode == "count" else self.int_type(), None, False)]
                if self.chance(0.4):
                    params.append((self.fresh("y"), self.int_type(), None, False))
                f = Fn(name, params, INT, "spec")
            f.measure = mode
            group.append(f)
        for f in group:
            self.fns[f.name] = f
        o = self.o
        for f in group:
            ps = ", ".join(f"{p}: {t}" for p, t, _, _ in f.params)
            o.emit(0, f"fn {f.name}({ps}) -> Int")
            env = {p: (t, False) for p, t, _, _ in f.params}
            renv = dict(env)
            renv["result"] = (INT, False)
            st_param = f.params[0][1] in self.structs
            if st_param and self.chance(0.6):
                # A bound that holds of real values of the struct (from its fields'
                # refinements), and so must not be used of values never built.
                o.spec(1, "ensures ", f.name, "ensures",
                       B(self.r.choice([">", ">=", "!="]), ("raw", "result"), ("lit", self.r.randint(-1, 2))))
            for _ in range(self.r.randint(0, 2)):
                if self.chance(0.3):
                    # A postcondition that calls a member of the group.
                    g = self.r.choice(group)
                    e = B(self.r.choice(["==", ">=", "<=", "!="]), ("raw", "result"), self.spec_call(g, env, True))
                else:
                    e = self.bool_expr(renv, 1, True)
                o.spec(1, "ensures ", f.name, "ensures", e)
            x = f.params[0][0]
            if f.measure == "count":
                o.emit(1, f"decreases {x}")
            else:
                o.emit(1, "decreases 0")
            o.emit(0, "do")
            # The body calls the group only where its measure allows.
            for g in group:
                g.kind = "pending"
            partner = self.r.choice(group)
            if f.measure == "count":
                # Bounded, so that nested calls (f(f(...))) cannot recurse millions deep.
                o.emit(1, f"if ({x} > 0) and ({x} < 40) then")
                rest = [(q, self.int_expr(env, 2)) for q, _, _, _ in partner.params[1:]]
                good = [(partner.params[0][0], B("-", ("var", x), ("lit", 1)))] + rest
                r = self.fresh("r")
                line = f"let {r} = {render(('call', partner.name, good))}"
                if self.chance(0.5):
                    # An argument that may not decrease the measure: if the verifier
                    # rejects the call, it becomes x - 1.
                    arg = self.r.choice([("var", x), B("+", ("var", x), ("lit", 1)), B("-", ("var", x), ("lit", 2)),
                                         B("*", ("var", x), ("lit", 2)), B("/", ("var", x), ("lit", 2)),
                                         self.int_expr(env, 1)])
                    bad = [(partner.params[0][0], arg)] + rest
                    o.choice(2, f.name, "call", f"let {r} = {render(('call', partner.name, bad))}", line)
                else:
                    o.emit(2, line)
                o.emit(2, f"return {r} + {render(self.int_expr(env, 2), False)}")
                o.emit(1, "end if")
            else:
                # Recursion that never runs: the function is axiomatised, not unfolded.
                o.emit(1, f"if {x} != {x} then" if self.is_int(f.params[0][1]) else
                       f"if {self.dead_cond(f)} then")
                o.emit(2, f"return {render(('call', f.name, [(p, ('var', p)) for p, _, _, _ in f.params]))}")
                o.emit(1, "end if")
            fs = [n for n, t, _ in self.structs.get(f.params[0][1], []) if self.is_int(t)]
            if fs and self.chance(0.6):
                o.emit(1, f"return {x}.{self.r.choice(fs)}")
            else:
                o.emit(1, f"return {render(self.int_expr(env, 1))}")
            o.emit(0, f"end fn {f.name}")
            o.emit(0, "")
            for g in group:
                g.kind = "spec"

    def dead_cond(self, f):
        p, st = f.params[0][0], f.params[0][1]
        fs = [n for n, t, _ in self.structs[st] if self.is_int(t)]
        if fs:
            return f"{p}.{fs[0]} != {p}.{fs[0]}"
        return "false"

    def spec_call(self, f, env, spec):
        """A call to a specification function with arguments from env."""
        args = []
        for p, t, _, _ in f.params:
            if self.is_int(t):
                if f.kind == "rec" and not spec:
                    args.append((p, ("lit", self.r.randint(0, 5))))
                else:
                    args.append((p, self.int_expr(env, 2, spec)))
            else:
                cands = [v for v, (vt, _) in env.items() if vt == t]
                if not cands:
                    return ("lit", self.small())
                args.append((p, ("var", self.r.choice(cands))))
        if len(args) == 1:
            return ("call", f.name, args)
        return ("call", f.name, args)

    def alias_names(self):
        for _ in range(self.r.randint(0, 2)):
            self.aliases[self.fresh("Al")] = None

    def alias_decls(self):
        """The aliases' refinements, which may call the specification functions that
        take the aliases (the shape of tests/fail/alias_cycle.vow)."""
        for name in sorted(self.aliases):
            pr = self.pred(("raw", "it"))
            self.aliases[name] = pr
            self.o.emit(0, f"type {name} = Int")
            self.o.spec(1, "where ", name, "alias", pr)
            self.o.emit(0, "")

    def struct_decls(self):
        for _ in range(self.r.randint(0, 2)):
            name = self.fresh("St")
            fields = []
            self.o.emit(0, f"struct {name}")
            for j in range(self.r.randint(1, 3)):
                fname = "f" + str(j)
                if j == 2 and self.chance(0.5):
                    al = self.elem_alias()
                    fields.append((fname, LIST, al))
                    self.o.emit(1, f"{fname}: List<{al or 'Int'}>")
                    if self.chance(0.6):
                        pr = B(self.r.choice([">", ">="]), ("len", ("raw", "it")), ("lit", self.r.randint(0, 2)))
                        self.o.spec(2, "where ", name, "field", pr, fname)
                    self.o.emit(1, ",")
                    continue
                t = self.int_type()
                fields.append((fname, t, None))
                self.o.emit(1, f"{fname}: {t}")
                if self.chance(0.6):
                    pr = self.pred(("raw", "it"), [f for f, ft, _ in fields[:-1] if self.is_int(ft)])
                    self.o.spec(2, "where ", name, "field", pr, fname)
                self.o.emit(1, ",")
            self.o.emit(0, f"end struct {name}")
            self.o.emit(0, "")
            self.structs[name] = fields

    def enum_decl(self):
        if self.chance(0.6):
            name = self.fresh("En")
            vs = [("Va", ["p"]), ("Vb", ["q", "s"]), ("Vc", [])]
            self.enum = (name, vs)
            self.o.emit(0, f"enum {name}")
            for v, fs in vs:
                self.o.emit(1, v + ("(" + ", ".join(f + ": Int" for f in fs) + ")" if fs else "") + ",")
            self.o.emit(0, f"end enum {name}")
            self.o.emit(0, "")
        if self.chance(0.5):
            self.tree = True
            self.o.emit(0, "enum Tree")
            self.o.emit(1, "Leaf,")
            self.o.emit(1, "Node(left: Tree, value: Int, right: Tree),")
            self.o.emit(0, "end enum Tree")
            self.o.emit(0, "")
            # Recursive specification functions over the tree.
            f = Fn("size", [("t", TREE, None, False)], INT, "rec")
            self.o.emit(0, "fn size(t: Tree) -> Int")
            self.o.spec(1, "ensures ", "size", "ensures", B(">=", ("raw", "result"), ("lit", self.r.choice([0, 0, 1]))))
            self.o.emit(1, "decreases t")
            self.o.emit(0, "do")
            self.o.emit(1, "match t")
            self.o.emit(1, "case Leaf then")
            self.o.emit(2, "return 0")
            self.o.emit(1, "case Node(left: l, value: v, right: r) then")
            self.o.emit(2, "return (size(l) + 1) + size(r)")
            self.o.emit(1, "end match")
            self.o.emit(0, "end fn size")
            self.o.emit(0, "")
            self.fns["size"] = f
            f = Fn("total", [("t", TREE, None, False)], INT, "rec")
            self.o.emit(0, "fn total(t: Tree) -> Int")
            if self.chance(0.5):
                self.o.spec(1, "ensures ", "total", "ensures",
                            B("<=", ("raw", "result"), B("*", ("lit", self.small(0, 40)), ("call", "size", [("t", ("raw", "t"))]))))
            self.o.emit(1, "decreases t")
            self.o.emit(0, "do")
            self.o.emit(1, "match t")
            self.o.emit(1, "case Leaf then")
            self.o.emit(2, "return 0")
            self.o.emit(1, "case Node(left: l, value: v, right: r) then")
            self.o.emit(2, "return (total(l) + v) + total(r)")
            self.o.emit(1, "end match")
            self.o.emit(0, "end fn total")
            self.o.emit(0, "")
            self.fns["total"] = f

    def rec_fns(self):
        for _ in range(self.r.randint(0, 2)):
            name = self.fresh("rec")
            n = self.fresh("n")
            f = Fn(name, [(n, INT, None, False)], INT, "rec")
            base, step = self.small(-2, 5), self.small(-2, 4)
            self.o.emit(0, f"fn {name}({n}: Int) -> Int")
            self.o.emit(1, f"requires {n} >= 0")
            if self.chance(0.6):
                self.o.spec(1, "ensures ", name, "ensures",
                            B(self.r.choice([">=", "<=", ">"]), ("raw", "result"),
                              self.r.choice([("lit", self.small(-3, 6)), B("*", ("var", n), ("lit", step)),
                                             B("+", ("var", n), ("lit", base))])))
            self.o.emit(1, f"decreases {n}")
            self.o.emit(0, "do")
            self.o.emit(1, f"if {n} == 0 then")
            self.o.emit(2, f"return {render(('lit', base))}")
            self.o.emit(1, "end if")
            if self.chance(0.4):
                m = self.fresh("m")
                self.o.emit(1, f"let {m} = {name}({n}: {n} - 1)")
                self.o.emit(1, f"return {m} + {render(('lit', step))}")
            else:
                self.o.emit(1, f"return {name}({n}: {n} - 1) + {render(('lit', step))}")
            self.o.emit(0, f"end fn {name}")
            self.o.emit(0, "")
            self.fns[name] = f

    # -- expressions -----------------------------------------------------------

    def vars_of(self, env, pred):
        return [v for v, (t, mut) in env.items() if pred(t)]

    def int_expr(self, env, d=0, spec=False):
        ints = self.vars_of(env, self.is_int)
        lists = self.vars_of(env, lambda t: t == LIST)
        structs = self.vars_of(env, lambda t: t in self.structs)
        if d > 2 or self.chance(0.3):
            if ints and self.chance(0.7):
                return ("var", self.r.choice(ints))
            return ("lit", self.small())
        choices = ["lit", "var", "var", "add", "sub", "mul", "div", "mod"]
        if lists:
            choices += ["len"]
        if structs:
            choices += ["field", "field"]
        if [f for f in self.fns.values() if f.kind == "expr" and f.ret == INT]:
            choices += ["mix"]
        if [f for f in self.fns.values() if f.kind == "rec" and f.params[0][1] == INT]:
            choices += ["rec"]
        if self.callable_specs(env):
            choices += ["spec", "spec"]
        if self.tree and self.vars_of(env, lambda t: t == TREE):
            choices += ["tree"]
        c = self.r.choice(choices)
        if c == "lit" or (c == "var" and not ints):
            return ("lit", self.small())
        if c == "var":
            return ("var", self.r.choice(ints))
        if c in ("add", "sub"):
            return B("+" if c == "add" else "-", self.int_expr(env, d + 1, spec), self.int_expr(env, d + 1, spec))
        if c == "mul":
            return B("*", self.int_expr(env, d + 1, spec), ("lit", self.small(-2, 3)))
        if c in ("div", "mod"):
            return B("/" if c == "div" else "%", self.int_expr(env, d + 1, spec),
                     ("lit", self.r.choice([1, 2, 3, -2, -3, 7])))
        if c == "len":
            return ("len", ("var", self.r.choice(lists)))
        if c == "field":
            s = self.r.choice(structs)
            fs = [f for f, t, _ in self.structs[env[s][0]] if self.is_int(t)]
            if fs:
                return ("field", ("var", s), self.r.choice(fs))
            return ("lit", self.small())
        if c == "mix":
            f = self.r.choice([f for f in self.fns.values() if f.kind == "expr" and f.ret == INT])
            return ("call", f.name, [(p, self.int_expr(env, d + 1, spec)) for p, _, _, _ in f.params])
        if c == "rec":
            f = self.r.choice([f for f in self.fns.values() if f.kind == "rec" and f.params[0][1] == INT])
            # The argument must be provably non-negative: a length, a small literal, or
            # (in a specification, where an undefined use is just removed) anything.
            arg = self.r.choice([("lit", self.r.randint(0, 5))] +
                                ([("len", ("var", self.r.choice(lists)))] if lists else []) +
                                ([self.int_expr(env, d + 1, spec)] if spec else []))
            return ("call", f.name, [(f.params[0][0], arg)])
        if c == "spec":
            return self.spec_call(self.r.choice(self.callable_specs(env)), env, spec)
        if c == "tree":
            t = self.r.choice(self.vars_of(env, lambda t: t == TREE))
            return ("call", self.r.choice(["size", "total"]), [("t", ("var", t))])
        return ("lit", 0)

    def callable_specs(self, env):
        types = {t for t, _ in env.values()}
        return [f for f in self.fns.values() if f.kind == "spec" and
                all(self.is_int(t) or t in types for _, t, _, _ in f.params)]

    def bool_expr(self, env, d=0, spec=False):
        r = self.r.random()
        if d < 2 and r < 0.25:
            return B(self.r.choice(["and", "or"]), self.bool_expr(env, d + 1, spec), self.bool_expr(env, d + 1, spec))
        if d < 2 and r < 0.32:
            return ("not", self.bool_expr(env, d + 1, spec))
        if spec and d < 2 and r < 0.40:
            return B("implies", self.bool_expr(env, d + 1, spec), self.bool_expr(env, d + 1, spec))
        if spec and r < 0.50:
            q = self.quant(env)
            if q:
                return q
        bools = self.vars_of(env, lambda t: t == BOOL)
        if bools and r < 0.55:
            return ("var", self.r.choice(bools))
        opts = self.vars_of(env, lambda t: t == OPT)
        if opts and r < 0.6:
            return ("call", self.r.choice(["is_some", "is_none"]), [("", ("var", self.r.choice(opts)))])
        bool_fns = [f for f in self.fns.values() if f.kind == "expr" and f.ret == BOOL]
        if bool_fns and r < 0.68:
            f = self.r.choice(bool_fns)
            return ("call", f.name, [(f.params[0][0], self.int_expr(env, d + 1, spec))])
        return B(self.r.choice(["<", "<=", ">", ">=", "==", "!="]), self.int_expr(env, d + 1, spec),
                 self.int_expr(env, d + 1, spec))

    def quant(self, env):
        lists = self.vars_of(env, lambda t: t == LIST)
        k = self.fresh("k")
        inner = dict(env)
        inner[k] = (INT, False)
        if lists and self.chance(0.7):
            xs = ("var", self.r.choice(lists))
            lo = ("lit", 0) if self.chance(0.7) else ("lit", 1)
            body = B(self.r.choice(["<", "<=", ">", ">=", "!="]), ("idx", xs, ("var", k)),
                     self.r.choice([("lit", self.small()), self.int_expr(env, 2, True)]))
            return ("quant", self.r.choice(["forall", "forall", "exists"]), k, lo, ("len", xs), body)
        hi = self.int_expr(env, 2, True)
        return ("quant", self.r.choice(["forall", "exists"]), k, ("lit", self.small(-1, 2)), hi,
                self.bool_expr(inner, 2, True))

    def list_expr(self, env):
        lists = self.vars_of(env, lambda t: t == LIST)
        r = self.r.random()
        if lists and r < 0.35:
            return ("call", "push", [("list", ("var", self.r.choice(lists))), ("item", self.int_expr(env, 1))])
        if lists and r < 0.45:
            xs = ("var", self.r.choice(lists))
            return ("raw", f"slice(of: {render(xs)}, from: 0, to: len({render(xs)}))")
        if lists and r < 0.55:
            return ("var", self.r.choice(lists))
        n = self.r.randint(0, 4)
        if n == 0:
            # An empty list whose type needs no annotation.
            return ("raw", "slice(of: [0], from: 0, to: 0)")
        return ("raw", "[" + ", ".join(render(self.int_expr(env, 2)) for _ in range(n)) + "]")

    def tree_expr(self, env):
        trees = self.vars_of(env, lambda t: t == TREE)
        if trees and self.chance(0.6):
            return ("raw", f"Tree.Node(left: {self.r.choice(trees)}, value: {render(self.int_expr(env, 2))}, "
                           f"right: {self.r.choice(trees + ['Tree.Leaf'])})")
        if self.chance(0.5):
            return ("raw", "Tree.Leaf")
        return ("raw", f"Tree.Node(left: Tree.Leaf, value: {render(self.int_expr(env, 2))}, right: Tree.Leaf)")

    def struct_args(self, sname, env):
        fields = self.structs[sname]
        args = []
        for f, t, _ in fields:
            if t == LIST:
                args.append((f, self.list_expr(env)))
            else:
                args.append((f, self.int_expr(env, 1)))
        return args

    def struct_ctor(self, sname, args):
        return ("raw", sname + "(" + ", ".join(f + ": " + render(a) for f, a in args) + ")")

    # -- statements --------------------------------------------------------------

    def block(self, ind, env, depth, fn, loop=None):
        n = self.r.randint(1, 4) if depth < 2 else self.r.randint(1, 2)
        for _ in range(n):
            self.stmt(ind, env, depth, fn, loop)

    def assert_stmt(self, ind, env, fn):
        self.o.spec(ind, "assert ", fn.name, "assert", self.bool_expr(env, 0, True))

    def stmt(self, ind, env, depth, fn, loop):
        kinds = ["let", "let", "assert", "assert", "set", "if", "call", "float"]
        if depth < 2:
            kinds += ["for", "while", "foreach", "if"]
        if self.enum or self.tree:
            kinds += ["match"]
        if self.structs:
            kinds += ["struct", "with"]
        if loop is not None:
            kinds += ["break"]
        k = self.r.choice(kinds)
        o = self.o
        mutable_ints = [v for v, (t, m) in env.items() if m and t == INT]
        if k == "let":
            name = self.fresh("v")
            r = self.r.random()
            if r < 0.5:
                t = self.int_type()
                e = self.int_expr(env)
                mut = t == INT and self.chance(0.5)
                if t != INT:
                    # An alias-typed binding: guard it with the alias's refinement,
                    # unless the refinement was removed.
                    o.emit(ind, f"let {name}: {t} = {render(e)}")
                    env[name] = (t, False)
                    return
                o.emit(ind, f"{'var' if mut else 'let'} {name} = {render(e)}")
                env[name] = (INT, mut)
            elif r < 0.7:
                mut = self.chance(0.5)
                o.emit(ind, f"{'var' if mut else 'let'} {name}: List<{self.elem_alias() or 'Int'}> = {render(self.list_expr(env))}")
                env[name] = (LIST, mut)
            elif r < 0.8:
                o.emit(ind, f"let {name} = {render(self.bool_expr(env))}")
                env[name] = (BOOL, False)
            elif r < 0.9 and self.tree:
                mut = self.chance(0.4)
                o.emit(ind, f"{'var' if mut else 'let'} {name}: Tree = {render(self.tree_expr(env))}")
                env[name] = (TREE, mut)
            else:
                if self.chance(0.5):
                    o.emit(ind, f"let {name}: Option<{self.elem_alias() or 'Int'}> = Option.Some(value: {render(self.int_expr(env))})")
                else:
                    o.emit(ind, f"let {name}: Option<Int> = Option.None")
                env[name] = (OPT, False)
        elif k == "assert":
            self.assert_stmt(ind, env, fn)
        elif k == "set":
            muts = [v for v, (t, m) in env.items() if m]
            if not muts:
                return self.assert_stmt(ind, env, fn)
            v = self.r.choice(muts)
            t = env[v][0]
            if t == INT:
                o.emit(ind, f"set {v} = {render(self.int_expr(env))}")
            elif t == LIST:
                o.emit(ind, f"set {v} = {render(self.list_expr(env))}")
            elif t == TREE:
                o.emit(ind, f"set {v} = {render(self.tree_expr(env))}")
            elif t in self.structs:
                self.with_update(ind, env, v)
            else:
                self.assert_stmt(ind, env, fn)
        elif k == "if":
            o.emit(ind, f"if {render(self.bool_expr(env))} then")
            self.block(ind + 1, dict(env), depth + 1, fn, loop)
            if self.chance(0.3):
                o.emit(ind, f"elif {render(self.bool_expr(env))} then")
                self.block(ind + 1, dict(env), depth + 1, fn, loop)
            if self.chance(0.5):
                o.emit(ind, "else")
                self.block(ind + 1, dict(env), depth + 1, fn, loop)
            o.emit(ind, "end if")
            if self.chance(0.4):
                self.assert_stmt(ind, env, fn)
        elif k == "for":
            i = self.fresh("i")
            lists = self.vars_of(env, lambda t: t == LIST)
            if lists and self.chance(0.5):
                xs = self.r.choice(lists)
                lo, hi = ("lit", 0), ("len", ("var", xs))
            else:
                lo, hi = ("lit", self.small(-2, 2)), ("lit", self.small(0, 6))
            inner = dict(env)
            inner[i] = (INT, False)
            o.emit(ind, f"for {i} in {render(lo, False)}..{render(hi, False)}")
            for _ in range(self.r.randint(0, 2)):
                o.spec(ind + 1, "invariant ", fn.name, "invariant", self.bool_expr(inner, 0, True))
            o.emit(ind, "do")
            if hi[0] == "len" and not env[hi[1][1]][1] and self.chance(0.6):
                v = self.fresh("e")
                o.emit(ind + 1, f"let {v} = {hi[1][1]}[{i}]")
                inner[v] = (INT, False)
            self.block(ind + 1, inner, depth + 1, fn, loop="for")
            o.emit(ind, "end for")
            if self.chance(0.5):
                self.assert_stmt(ind, env, fn)
        elif k == "foreach":
            lists = self.vars_of(env, lambda t: t == LIST)
            if not lists:
                return self.assert_stmt(ind, env, fn)
            x = self.fresh("x")
            inner = dict(env)
            inner[x] = (INT, False)
            o.emit(ind, f"for {x} in {self.r.choice(lists)}")
            for _ in range(self.r.randint(0, 2)):
                o.spec(ind + 1, "invariant ", fn.name, "invariant", self.bool_expr(inner, 0, True))
            o.emit(ind, "do")
            self.block(ind + 1, inner, depth + 1, fn, loop="for")
            o.emit(ind, "end for")
        elif k == "while":
            w = self.fresh("w")
            bound = self.small(0, 7)
            o.emit(ind, f"var {w} = {render(('lit', self.small(-2, 2)))}")
            inner = dict(env)
            inner[w] = (INT, False)  # the body may not set the counter
            o.emit(ind, f"while {w} < {bound}")
            for _ in range(self.r.randint(0, 2)):
                o.spec(ind + 1, "invariant ", fn.name, "invariant", self.bool_expr(inner, 0, True))
            if self.chance(0.7):
                o.spec(ind + 1, "decreases ", fn.name, "decreases",
                       self.r.choice([B("-", ("lit", bound), ("var", w)), B("-", ("lit", bound + 3), ("var", w)),
                                      ("var", w), self.int_expr(inner, 1, True),
                                      B("-", self.int_expr(inner, 0, True), ("var", w))]))
            o.emit(ind, "do")
            step = self.r.choice([1, 1, 2])
            if self.chance(0.3):
                o.emit(ind + 1, f"if {render(self.bool_expr(inner))} then")
                o.emit(ind + 2, f"set {w} = {w} + {step}")
                o.emit(ind + 2, "continue")
                o.emit(ind + 1, "end if")
            self.block(ind + 1, inner, depth + 1, fn, loop="while")
            o.emit(ind + 1, f"set {w} = {w} + {step}")
            o.emit(ind, "end while")
            env[w] = (INT, False)
            if self.chance(0.6):
                self.assert_stmt(ind, env, fn)
        elif k == "break":
            o.emit(ind, f"if {render(self.bool_expr(env))} then")
            o.emit(ind + 1, "break")
            o.emit(ind, "end if")
        elif k == "match":
            self.match_stmt(ind, env, depth, fn, loop)
        elif k == "struct":
            sname = self.r.choice(sorted(self.structs))
            args = self.struct_args(sname, env)
            name = self.fresh("s")
            mut = self.chance(0.5)
            conds = [s for s in self.o.specs if s.scope == sname and s.kind == "field"]
            if conds and self.chance(0.6):
                # Build it only where the refinements hold, as a guard in the code.
                o.emit(ind, f"if {self.guard_text(sname, args)} then")
                o.emit(ind + 1, f"{'var' if mut else 'let'} {name} = {render(self.struct_ctor(sname, args))}")
                inner = dict(env)
                inner[name] = (sname, mut)
                users = [f for f in self.fns.values() if f.kind == "spec" and f.params[0][1] == sname]
                if users and self.chance(0.8):
                    call = self.spec_call(self.r.choice(users), inner, True)
                    how = self.r.choice(["let", "assert", "measure"])
                    if how == "let":
                        u = self.fresh("u")
                        o.emit(ind + 1, f"let {u} = {render(call)}")
                        inner[u] = (INT, False)
                    elif how == "assert":
                        o.spec(ind + 1, "assert ", fn.name, "assert",
                               B(self.r.choice([">", ">=", "=="]), call, ("lit", self.r.randint(-1, 2))))
                    else:
                        # The struct's function in a loop measure: a specification
                        # that the verifier must not use to learn about other paths.
                        w = self.fresh("w")
                        o.emit(ind + 1, f"var {w} = 0")
                        o.emit(ind + 1, f"while {w} < 1")
                        o.spec(ind + 2, "decreases ", fn.name, "decreases", B("-", call, ("var", w)))
                        o.emit(ind + 1, "do")
                        o.emit(ind + 2, f"set {w} = 1")
                        o.emit(ind + 1, "end while")
                self.block(ind + 1, inner, depth + 1, fn, loop)
                o.emit(ind, "end if")
                if self.chance(0.6):
                    # What the guard established, which is false where the branch did
                    # not run (the shape of tests/fail/unreal_struct.vow).
                    c = self.r.choice(conds)
                    fenv = {f: a for f, a in args}
                    e = subst(c.text, dict(fenv, it=fenv[c.extra]))
                    o.spec(ind, "assert ", fn.name, "assert", e)
            else:
                o.emit(ind, f"{'var' if mut else 'let'} {name} = {render(self.struct_ctor(sname, args))}")
                env[name] = (sname, mut)
        elif k == "with":
            muts = [v for v, (t, m) in env.items() if m and t in self.structs]
            if muts:
                self.with_update(ind, env, self.r.choice(muts))
            else:
                self.assert_stmt(ind, env, fn)
        elif k == "call":
            self.call_stmt(ind, env, fn)
        elif k == "float":
            self.float_assert(ind, fn)

    def guard_text(self, sname, args):
        # Rendered later, when the removed refinements are known: a placeholder.
        key = f"@G{len(self.guards)}@"
        self.guards.append((sname, args))
        return key

    def with_update(self, ind, env, v):
        sname = env[v][0]
        fs = [(f, t) for f, t, _ in self.structs[sname]]
        f, t = self.r.choice(fs)
        val = self.list_expr(env) if t == LIST else self.int_expr(env, 1)
        self.o.emit(ind, f"set {v} = {v} with ({f}: {render(val)})")

    def match_stmt(self, ind, env, depth, fn, loop):
        o = self.o
        trees = self.vars_of(env, lambda t: t == TREE)
        ens = self.vars_of(env, lambda t: self.enum and t == self.enum[0])
        opts = self.vars_of(env, lambda t: t == OPT)
        picks = ["tree"] * bool(trees) + ["enum"] * bool(ens) + ["opt"] * bool(opts) + ["mkenum"] * bool(self.enum)
        if not picks:
            return self.assert_stmt(ind, env, fn)
        pick = self.r.choice(picks)
        if pick == "tree":
            t = self.r.choice(trees)
            o.emit(ind, f"match {t}")
            o.emit(ind, "case Leaf then")
            self.assert_stmt(ind + 1, env, fn)
            l, v, r = self.fresh("l"), self.fresh("tv"), self.fresh("r")
            o.emit(ind, f"case Node(left: {l}, value: {v}, right: {r}) then")
            inner = dict(env)
            inner.update({l: (TREE, False), v: (INT, False), r: (TREE, False)})
            self.block(ind + 1, inner, depth + 1, fn, loop)
            o.emit(ind, "end match")
        elif pick == "opt":
            x = self.r.choice(opts)
            v = self.fresh("o")
            o.emit(ind, f"match {x}")
            o.emit(ind, f"case Some(value: {v}) then")
            inner = dict(env)
            inner[v] = (INT, False)
            self.block(ind + 1, inner, depth + 1, fn, loop)
            o.emit(ind, "case None then")
            self.assert_stmt(ind + 1, env, fn)
            o.emit(ind, "end match")
        else:
            name, vs = self.enum
            if pick == "mkenum" or not ens:
                v, fs = self.r.choice(vs)
                x = self.fresh("en")
                args = ", ".join(f + ": " + render(self.int_expr(env, 1)) for f in fs)
                o.emit(ind, f"let {x} = {name}.{v}" + (f"({args})" if fs else ""))
                env[x] = (name, False)
            else:
                x = self.r.choice(ens)
            o.emit(ind, f"match {x}")
            arms = list(vs)
            self.r.shuffle(arms)
            use_else = self.chance(0.3)
            if use_else:
                arms = arms[:self.r.randint(1, len(arms) - 1)]
            for v, fs in arms:
                inner = dict(env)
                binds = []
                for f in fs:
                    b = self.fresh("b")
                    binds.append(f"{f}: {b}")
                    inner[b] = (INT, False)
                o.emit(ind, f"case {v}" + ("(" + ", ".join(binds) + ")" if binds else "") + " then")
                self.block(ind + 1, inner, depth + 1, fn, loop)
            if use_else:
                o.emit(ind, "else")
                self.assert_stmt(ind + 1, env, fn)
            o.emit(ind, "end match")

    def call_stmt(self, ind, env, fn):
        callable_ = [f for f in self.fns.values() if f.kind == "plain" and f.name != fn.name and f.order < fn.order]
        if not callable_:
            return self.assert_stmt(ind, env, fn)
        g = self.r.choice(callable_)
        args = []
        for p, t, ref, inout in g.params:
            if inout:
                cands = [v for v, (vt, m) in env.items() if m and vt == t]
                if not cands:
                    return self.assert_stmt(ind, env, fn)
                args.append((p, None, self.r.choice(cands)))
            elif t == LIST:
                lv = self.fresh("l")
                self.o.emit(ind, f"let {lv}: List<Int> = {render(self.list_expr(env))}")
                env[lv] = (LIST, False)
                args.append((p, ("var", lv), None))
            elif t in self.structs:
                cands = [v for v, (vt, m) in env.items() if vt == t]
                if not cands:
                    return self.assert_stmt(ind, env, fn)
                args.append((p, ("var", self.r.choice(cands)), None))
            else:
                args.append((p, self.int_expr(env, 1), None))
        if len({a[2] for a in args if a[2]}) < len([a for a in args if a[2]]):
            return self.assert_stmt(ind, env, fn)
        call = g.name + "(" + ", ".join(p + ": " + (f"inout {v}" if v else render(a)) for p, a, v in args) + ")"
        key = f"@C{len(self.calls)}@"
        self.calls.append((g, {p: a for p, a, v in args if a is not None}, {p: v for p, a, v in args if v}))
        guarded = self.chance(0.85)
        if guarded:
            self.o.emit(ind, f"if {key} then")
            ind += 1
        if g.ret == "Unit":
            self.o.emit(ind, call)
        else:
            name = self.fresh("c")
            self.o.emit(ind, f"let {name} = {call}")
            if g.ret == INT:
                env2 = dict(env)
                env2[name] = (INT, False)
                if self.chance(0.6):
                    self.assert_stmt(ind, env2, fn)
                if not guarded:
                    env[name] = (INT, False)
        if guarded:
            self.o.emit(ind - 1, "end if")

    def float_assert(self, ind, fn):
        def lit():
            whole = str(self.r.randint(0, 3))
            frac = str(self.r.randint(0, 99))
            s = whole + "." + frac
            if self.chance(0.6):
                e = self.r.randint(0, 3)
                zeros = "0" * self.r.randint(0, 6)
                s += "e" + self.r.choice(["", "-", "+"]) + zeros + str(e)
            return s
        a, b = lit(), lit()
        self.o.spec(ind, "assert ", fn.name, "assert", ("bin", self.r.choice(["<", "<=", ">", "==", "!="]), ("raw", a), ("raw", b)))

    # -- functions ---------------------------------------------------------------

    def plain_fn(self, order):
        name = self.fresh("fn")
        params = []
        env = {}
        for _ in range(self.r.randint(1, 3)):
            p = self.fresh("p")
            t = self.int_type()
            params.append((p, t, None, False))
            env[p] = (t, False)
        if self.chance(0.4):
            p = self.fresh("xs")
            params.append((p, LIST, self.elem_alias(), False))
            env[p] = (LIST, False)
        if self.structs and self.chance(0.4):
            p = self.fresh("sp")
            t = self.r.choice(sorted(self.structs))
            params.append((p, t, None, False))
            env[p] = (t, False)
        if self.chance(0.4):
            p = self.fresh("io")
            t = self.r.choice([INT, INT, LIST] + sorted(self.structs))
            params.append((p, t, None, True))
            env[p] = (t, True)
        ret = self.r.choice([INT, INT, BOOL]) if not params[-1][3] or self.chance(0.5) else "Unit"
        f = Fn(name, params, ret, "plain")
        f.order = order
        self.fns[name] = f
        o = self.o
        o.emit(0, f"fn {name}(")
        for i, (p, t, ref, inout) in enumerate(params):
            shown = f"List<{ref}>" if t == LIST and ref else t
            o.emit(1, f"{p}: {'inout ' if inout else ''}{shown}")
            if t == INT and not inout and self.chance(0.3):
                pr = self.pred(("raw", "it"))
                o.spec(2, "where ", name, "param", pr, p)
            o.emit(1, ",")
        o.emit(0, ")" + ("" if ret == "Unit" else f" -> {ret}"))
        if ret == INT and self.chance(0.3):
            o.spec(1, "where ", name, "ret", self.pred(("raw", "it")))
        for _ in range(self.r.randint(0, 2)):
            e = self.bool_expr({p: (t, False) for p, t, _, io in params if not io}, 0, True)
            o.spec(1, "requires ", name, "requires", e)
        renv = {p: (t, False) for p, t, _, io in params}
        if ret == INT:
            renv["result"] = (INT, False)
        elif ret == BOOL:
            renv["result"] = (BOOL, False)
        for p, t, _, io in params:
            if io and t == INT:
                renv[f"old({p})"] = (INT, False)
            elif io and t == LIST:
                renv[f"old({p})"] = (LIST, False)
        for _ in range(self.r.randint(0, 3)):
            o.spec(1, "ensures ", name, "ensures", self.bool_expr(renv, 0, True))
        o.emit(0, "do")
        body_env = dict(env)
        self.block(1, body_env, 0, f)
        if ret == INT:
            o.emit(1, f"return {render(self.int_expr(body_env))}")
        elif ret == BOOL:
            o.emit(1, f"return {render(self.bool_expr(body_env))}")
        o.emit(0, f"end fn {name}")
        o.emit(0, "")

    def main(self):
        o = self.o
        o.emit(0, "fn main() uses console do")
        for g in [f for f in self.fns.values() if f.kind == "plain"]:
            sweeps = 2 if any(t == LIST or t in self.structs for _, t, _, _ in g.params) else 1
            for _ in range(sweeps):
                self.sweep(g)
        o.emit(1, 'print("done\\n")')
        o.emit(0, "end fn main")

    def lit_list(self, lo=0):
        n = self.r.randint(lo, 4)
        if n == 0:
            return ("raw", "slice(of: [0], from: 0, to: 0)")
        return ("raw", "[" + ", ".join(render(("lit", self.small())) for _ in range(n)) + "]")

    def sweep(self, g):
        """Calls g on every combination of small values for its Int parameters (and a
        few struct and list values), guarded by its preconditions."""
        o = self.o
        ints = [p for p, t, _, io in g.params if self.is_int(t)]
        width = {0: 0, 1: 8, 2: 5, 3: 3}.get(len(ints), 2)
        ind = 1
        loopvars = []
        for p in ints:
            v = self.fresh("a")
            lo = self.r.randint(-4, 0)
            o.emit(ind, f"for {v} in {render(('lit', lo))}..{lo + 2 * width + 1} do")
            ind += 1
            loopvars.append(v)
        if not loopvars:
            v = self.fresh("a")
            o.emit(ind, f"for {v} in -2..6 do")
            ind += 1
            loopvars.append(v)
        pool = loopvars
        args, inouts = {}, {}
        closers = []
        k = 0
        for p, t, _, io in g.params:
            if self.is_int(t):
                val = ("var", ints and loopvars[k] or loopvars[0])
                k += 1
            elif t == LIST:
                val = ("var", self.fresh("l"))
                o.emit(ind, f"let {val[1]}: List<Int> = {render(self.lit_list())}")
            else:
                fargs = []
                for f, ft, _ in self.structs[t]:
                    if ft == LIST:
                        fargs.append((f, self.lit_list(1)))
                    elif self.chance(0.7):
                        fargs.append((f, ("var", self.r.choice(pool))))
                    else:
                        fargs.append((f, ("lit", self.small())))
                o.emit(ind, f"if {self.guard_text(t, fargs)} then")
                closers.append(ind)
                ind += 1
                val = ("var", self.fresh("sv"))
                o.emit(ind, f"let {val[1]} = {render(self.struct_ctor(t, fargs))}")
            if io:
                v = self.fresh("m")
                o.emit(ind, f"var {v}: {t} = {render(val)}")
                inouts[p] = v
            else:
                args[p] = val
        self._finish_main_call(g, args, inouts, ind)
        for c in reversed(closers):
            o.emit(c, "end if")
        for i in range(len(loopvars)):
            ind -= 1
            o.emit(ind, "end for")

    def _finish_main_call(self, g, args, inouts, ind):
        call = g.name + "(" + ", ".join(
            p + ": " + (f"inout {inouts[p]}" if p in inouts else render(args[p])) for p, _, _, _ in g.params) + ")"
        key = f"@C{len(self.calls)}@"
        self.calls.append((g, args, inouts))
        self.o.emit(ind, f"if {key} then")
        if g.ret == "Unit":
            self.o.emit(ind + 1, call)
        elif g.ret == INT:
            self.o.emit(ind + 1, f'print(int_to_str({call}) ++ "\\n")')
        else:
            c = self.fresh("c")
            self.o.emit(ind + 1, f"let {c} = {call}")
            self.o.emit(ind + 1, f'if {c} then')
            self.o.emit(ind + 2, 'print("t\\n")')
            self.o.emit(ind + 1, "end if")
        self.o.emit(ind, "end if")

    def program(self):
        self.guards = []
        self.calls = []
        o = self.o
        o.emit(0, "// generated by tests/genfuzz.py")
        o.emit(0, "")
        self.expr_fns()
        self.alias_names()
        if self.chance(0.6):
            self.spec_group()
        self.alias_decls()
        self.struct_decls()
        if self.structs and self.chance(0.6):
            self.spec_group(structs=True)
        self.enum_decl()
        self.rec_fns()
        for i in range(self.r.randint(1, 4)):
            self.plain_fn(i)
        self.main()
        return self

    # -- rendering with contracts removed -------------------------------------------

    def source(self, off):
        text = self.o.text(off)
        on = [s for s in self.o.specs if s.sid not in off]

        def guard(m):
            k = int(m.group(1))
            sname, args = self.guards[k]
            env = {f: a for f, a in args}
            conds = []
            for s in on:
                if s.scope == sname and s.kind == "field":
                    fval = env[s.extra]
                    conds.append(subst(s.text, dict(env, it=fval)))
            for f, t, al in self.structs[sname]:
                c = self.elems_ok(al, env[f], on) if t == LIST else None
                if c:
                    conds.append(c)
            return render(self.conj(conds))

        def call_guard(m):
            k = int(m.group(1))
            g, args, inouts = self.calls[k]
            env = dict(args)
            for p, v in inouts.items():
                env[p] = ("var", v)
            conds = []
            # Alias-typed parameters carry the alias's refinement; the callee assumes
            # it before its preconditions, which may rely on it.
            for p, t, _, io in g.params:
                if t in self.aliases and not io:
                    for s in on:
                        if s.scope == t and s.kind == "alias":
                            conds.append(subst(s.text, {"it": env[p]}))
            for p, t, al, io in g.params:
                c = self.elems_ok(al, env[p], on) if t == LIST and p in env else None
                if c:
                    conds.append(c)
            for s in on:
                if s.scope == g.name and s.kind == "requires":
                    conds.append(subst(s.text, env))
                if s.scope == g.name and s.kind == "param":
                    conds.append(subst(s.text, dict(env, it=env[s.extra])))
            return render(self.conj(conds))

        text = re.sub(r"@G(\d+)@", guard, text)
        text = re.sub(r"@C(\d+)@", call_guard, text)
        return text

    def conj(self, conds):
        if not conds:
            return ("bool", True)
        return ("raw", " and ".join("(" + render(c) + ")" for c in conds))


def subst(text, env):
    """A contract's text with the variables in env replaced by expressions (for guards
    built from contracts), as an expression."""
    def rep(m):
        name = m.group(0)
        if name.rstrip().endswith(":"):
            return name
        return "(" + render(env[name]) + ")" if name in env else name
    # Leave argument labels ('(' or ',' then 'name:') and field names ('.name') alone.
    return ("raw", re.sub(r"(?<=[(,])\s*[a-z_][A-Za-z0-9_]*\s*:|(?<![\w)\]]\.)\b[a-z_][A-Za-z0-9_]*\b",
                          rep, text))


# ---------------------------------------------------------------------------
# Removing what the verifier rejects.

ERR = re.compile(r"^(?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+): error: (?P<msg>.*)$")
NOTE_FN = re.compile(r"in function '([^']+)'")


def diagnostics(stderr):
    out = []
    cur = None
    for line in stderr.splitlines():
        m = ERR.match(line)
        if m:
            cur = {"line": int(m.group("line")), "msg": m.group("msg"), "fn": None}
            out.append(cur)
        elif cur is not None:
            n = NOTE_FN.search(line)
            if n:
                cur["fn"] = n.group(1)
    return out


def to_remove(g, d, off):
    """The contracts to remove for one diagnostic (empty if none can be blamed)."""
    msg, fn = d["msg"], d["fn"]
    on = [s for s in g.o.specs if s.sid not in off]
    hit = [s for s in on if s.line == d["line"]]
    if hit:
        return {s.sid for s in hit}
    m = re.match(r"cannot prove (precondition|postcondition) of '([^']+)': (.*)$", msg)
    if m:
        kind = "requires" if m.group(1) == "precondition" else "ensures"
        t = norm(m.group(3))
        hit = [s for s in on if s.scope == m.group(2) and s.kind == kind and norm(s.text) == t]
        return {s.sid for s in hit} or {s.sid for s in on if s.scope == m.group(2) and s.kind == kind}
    m = re.match(r"cannot prove return value of '([^']+)' satisfies", msg)
    if m:
        hit = [s for s in on if s.scope == m.group(1) and s.kind == "ret"]
        if hit:
            return {s.sid for s in hit}
    m = re.match(r"cannot prove argument '([^']+)' of '([^']+)' satisfies (.*)$", msg)
    if m:
        hit = [s for s in on if s.scope == m.group(2) and s.kind == "param" and s.extra == m.group(1)]
        if hit:
            return {s.sid for s in hit}
    m = re.match(r"cannot prove field '([^']+)' of (\S+) satisfies", msg)
    if m:
        hit = [s for s in on if s.scope == m.group(2) and s.kind == "field" and s.extra == m.group(1)]
        if hit:
            return {s.sid for s in hit}
    m = re.search(r"satisfies (.*)$", msg)
    if m:
        t = norm(m.group(1))
        hit = [s for s in on if s.kind in ("alias", "param", "ret", "field") and norm(s.text) == t]
        if hit:
            return {s.sid for s in hit}
    # Otherwise, a contract of this function whose text the message quotes.
    m = re.search(r"cannot prove '(.*)' is defined", msg)
    texts = [m.group(1)] if m else []
    m = re.search(r": (.*)$", msg)
    if m:
        texts.append(m.group(1))
    m = re.match(r"cannot prove loop invariant (.*) (is preserved|holds at .*)$", msg)
    if m:
        texts.append(m.group(1))
    for t in texts:
        t = norm(t)
        hit = [s for s in on if (fn is None or s.scope == fn) and norm(s.text) == t]
        if not hit:
            hit = [s for s in on if norm(s.text) == t]
        if hit:
            return {s.sid for s in hit}
    return set()


def attempt(job):
    i, seed, vowc, base, rounds, timeout = job
    g = Gen(random.Random(seed)).program()
    work = os.path.join(base, f"g{i}")
    os.makedirs(work)
    path = os.path.join(work, "gen.vow")
    off = set()
    try:
        for _ in range(rounds):
            src = g.source(off)
            with open(path, "w") as f:
                f.write(src)
            code, out, err = run([vowc, "check", "gen.vow", "--no-cache"], work, timeout)
            if code is None:
                return "timeout", seed, None, src
            if code == 0:
                break
            ds = diagnostics(err)
            if not ds:
                return "invalid", seed, err.strip().splitlines()[:3], src
            # A diagnostic that blames no contract may go away once others are
            # removed (a guard built from a removed precondition, say).
            drop = set()
            for d in ds:
                drop |= to_remove(g, d, off)
            if not drop:
                return "stuck", seed, ds[0], src
            off |= drop
        else:
            return "stuck", seed, "too many rounds", src
        code, out, err = run([vowc, "run", "gen.vow", "--check-contracts", "--no-cache"], work, timeout)
        bug = is_bug(err) if code is not None else None
        if bug:
            return "BUG", seed, bug, src
        if code is None or (code != 0 and "vow: panic" not in err):
            # Checking contracts must not change whether a program finishes: if the
            # plain build runs quickly, the checks diverged or crashed.
            pcode, _, perr = run([vowc, "run", "gen.vow", "--no-cache"], work, max(5, timeout // 6))
            if pcode == 0:
                what = "timed out" if code is None else f"exited with {code}"
                return "BUG", seed, f"the build with --check-contracts {what}; the plain build ran", src
            if code is None:
                return "timeout", seed, None, src
        if code != 0:
            return ("panic" if "vow: panic" in err else "other"), seed, err.strip().splitlines()[:3], src
        kept = len(g.o.specs) - len(off)
        return "verified", seed, kept, src
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--count", type=int, default=200)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--out", default="fuzz-findings")
    ap.add_argument("--keep", default=None)
    ap.add_argument("--vowc", default=os.path.join(ROOT, "build", "vowc"))
    ap.add_argument("--timeout", type=int, default=60)
    ap.add_argument("--rounds", type=int, default=12)
    ap.add_argument("--show", default=None, help="print the program for one seed and exit")
    args = ap.parse_args()
    if args.show is not None:
        print(Gen(random.Random(int(args.show))).program().source(set()), end="")
        return 0
    vowc = os.path.abspath(args.vowc)
    base = tempfile.mkdtemp(prefix="vow-genfuzz-")
    jobs = [(i, args.seed * 1_000_003 + i, vowc, base, args.rounds, args.timeout) for i in range(args.count)]
    counts = {}
    findings = 0
    kept_specs = 0
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as ex:
            for kind, seed, info, src in ex.map(attempt, jobs):
                counts[kind] = counts.get(kind, 0) + 1
                if kind == "verified":
                    kept_specs += info
                    if args.keep:
                        os.makedirs(args.keep, exist_ok=True)
                        with open(os.path.join(args.keep, f"gen_{seed}.vow"), "w") as f:
                            f.write(src)
                if kind in ("invalid", "stuck", "other", "timeout") and os.environ.get("GENFUZZ_DEBUG"):
                    print(f"{kind} seed {seed}: {info}", file=sys.stderr)
                if kind == "BUG":
                    findings += 1
                    os.makedirs(args.out, exist_ok=True)
                    saved = os.path.join(args.out, f"gen_{seed}.vow")
                    with open(saved, "w") as f:
                        f.write(f"// genfuzz finding (seed {seed})\n// {info}\n" + src)
                    print(f"BUG seed {seed}: {info} (saved {saved})", flush=True)
        print(", ".join(f"{v} {k}" for k, v in sorted(counts.items())) +
              f"; {kept_specs} contracts proved and checked at runtime")
        return 1 if findings else 0
    finally:
        shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
