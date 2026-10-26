// Verification condition generation by symbolic execution, discharged with Z3.
//
// Every value gets an SMT term. Definitions of fresh constants (`v7 = v3 + 1`) are
// asserted unconditionally: a fresh constant can always be defined, so they cannot make
// anything inconsistent. A constraint on a fresh constant's *properties* is only global
// when it is satisfiable regardless of the path (`len(r) = len(a) + 1` is; `len(r) =
// to - from` is not, because of the `len >= 0` axiom). Everything that is only true on some paths (branch conditions,
// callee postconditions, `check`) lives in the path's `facts`, and each proof
// obligation is checked as `facts and not goal`.

use crate::ast::*;
use crate::check::{builtin, CallKind, Info, RTy, Ty};
use crate::diag::{Diag, Pos};
use crate::pretty;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug)]
pub struct Ob {
    pub pos: Pos,
    pub what: String,
    pub facts: Vec<String>,
    pub goal: String,
    /// (display name, SMT term) pairs to show in a counterexample.
    pub show: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct FnVc {
    pub name: String,
    pub decls: Vec<String>,
    pub defs: Vec<String>,
    /// Quantified content axioms about fresh constants. They are consistent by
    /// construction (each only describes a fresh constant's contents), and Z3 cannot
    /// cheaply show their satisfiability, so they come after the vacuity check.
    pub qdefs: Vec<String>,
    pub obs: Vec<Ob>,
}

#[derive(Clone)]
struct Var {
    term: String,
    ty: Ty,
    preds: Vec<Expr>,
}

#[derive(Clone)]
struct St {
    env: HashMap<String, Var>,
    facts: Vec<String>,
    src: Vec<String>,
    dead: bool,
}

struct Spec {
    binds: HashMap<String, String>,
    it: Option<String>,
    result: Option<String>,
}

struct Loop {
    invs: Vec<Expr>,
    /// For a range loop: the loop variable and its value in the next iteration, so
    /// that invariants mentioning it are checked for the next iteration at `continue`.
    next: Option<(String, String)>,
    /// The loop's `decreases` measure and its value at the top of the iteration.
    dec: Option<(Expr, String)>,
}

pub struct Verifier<'a> {
    info: &'a Info,
    n: usize,
    decls: Vec<String>,
    defs: Vec<String>,
    qdefs: Vec<String>,
    obs: Vec<Ob>,
    pub preamble: BTreeSet<String>,
    strs: HashMap<String, String>,
    loops: Vec<Loop>,
    fname: String,
    ret: RTy,
    ensures: Vec<Expr>,
    decreases: Option<Expr>,
    params: HashMap<String, String>,
    pub hole_facts: HashMap<ExprId, Vec<String>>,
}

pub fn sort(t: &Ty) -> &'static str {
    match t {
        Ty::Int | Ty::Unit => "Int",
        Ty::Bool => "Bool",
        _ => "Val",
    }
}

const PRELUDE: &[&str] = &[
    "(declare-sort Val 0)",
    "(declare-fun len (Val) Int)",
    "(declare-fun byte (Val Int) Int)",
    "(declare-fun tag (Val) Int)",
    "(declare-fun elem_Int (Val Int) Int)",
    "(declare-fun elem_Bool (Val Int) Bool)",
    "(declare-fun elem_Val (Val Int) Val)",
    "(assert (forall ((x Val)) (! (>= (len x) 0) :pattern ((len x)))))",
    "(assert (forall ((s Val) (i Int)) (! (and (>= (byte s i) 0) (< (byte s i) 256)) :pattern ((byte s i)))))",
];

fn and_all(v: &[String]) -> String {
    match v.len() {
        0 => "true".into(),
        1 => v[0].clone(),
        _ => format!("(and {})", v.join(" ")),
    }
}

fn int_lit(n: i64) -> String {
    if n < 0 {
        format!("(- {})", (n as i128).abs())
    } else {
        n.to_string()
    }
}

fn assigned_expr(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        EK::Call(c, args) | EK::With(c, args) => {
            assigned_expr(c, out);
            for a in args {
                if let (true, EK::Var(n)) = (a.inout, &a.value.kind) {
                    if !out.contains(n) {
                        out.push(n.clone());
                    }
                }
                assigned_expr(&a.value, out);
            }
        }
        EK::List(items) => {
            for i in items {
                assigned_expr(i, out);
            }
        }
        EK::Index(a, b) | EK::Binary(_, a, b) => {
            assigned_expr(a, out);
            assigned_expr(b, out);
        }
        EK::Field(a, _) | EK::Unary(_, a) => assigned_expr(a, out),
        EK::Quant(_, _, lo, hi, b) => {
            assigned_expr(lo, out);
            assigned_expr(hi, out);
            assigned_expr(b, out);
        }
        _ => {}
    }
}

/// Variables assigned with `set`, or passed inout, anywhere in a block.
fn assigned(stmts: &[Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match &s.kind {
            SK::Set { name, value } => {
                if !out.contains(name) {
                    out.push(name.clone());
                }
                assigned_expr(value, out);
            }
            SK::Let { value, .. } | SK::Return(Some(value)) | SK::Expr(value) | SK::Check(value) => {
                assigned_expr(value, out)
            }
            SK::If { arms, els } => {
                for (c, b) in arms {
                    assigned_expr(c, out);
                    assigned(b, out);
                }
                if let Some(b) = els {
                    assigned(b, out);
                }
            }
            SK::While { cond, body, .. } => {
                assigned_expr(cond, out);
                assigned(body, out);
            }
            SK::ForRange { lo, hi, body, .. } => {
                assigned_expr(lo, out);
                assigned_expr(hi, out);
                assigned(body, out);
            }
            SK::ForEach { list, body, .. } => {
                assigned_expr(list, out);
                assigned(body, out);
            }
            SK::Match { scrut, cases, els } => {
                assigned_expr(scrut, out);
                for c in cases {
                    assigned(&c.body, out);
                }
                if let Some(b) = els {
                    assigned(b, out);
                }
            }
            _ => {}
        }
    }
}

impl<'a> Verifier<'a> {
    pub fn new(info: &'a Info) -> Verifier<'a> {
        Verifier {
            info,
            n: 0,
            decls: Vec::new(),
            defs: Vec::new(),
            qdefs: Vec::new(),
            obs: Vec::new(),
            preamble: PRELUDE.iter().map(|s| s.to_string()).collect(),
            strs: HashMap::new(),
            loops: Vec::new(),
            fname: String::new(),
            ret: RTy::plain(Ty::Unit),
            ensures: Vec::new(),
            decreases: None,
            params: HashMap::new(),
            hole_facts: HashMap::new(),
        }
    }

    pub fn prelude() -> Vec<String> {
        PRELUDE.iter().map(|s| s.to_string()).collect()
    }

    fn fresh(&mut self, sort: &str) -> String {
        self.n += 1;
        let v = format!("v{}", self.n);
        self.decls.push(format!("(declare-const {} {})", v, sort));
        v
    }

    fn define(&mut self, sort: &str, term: String) -> String {
        let v = self.fresh(sort);
        self.defs.push(format!("(assert (= {} {}))", v, term));
        v
    }

    fn global_fn(&mut self, name: &str, args: &str, sort: &str) -> String {
        self.preamble.insert(format!("(declare-fun {} ({}) {})", name, args, sort));
        name.to_string()
    }

    fn field_fn(&mut self, sname: &str, field: &str, ty: &Ty) -> String {
        let n = format!("fld_{}_{}", sname, field);
        self.global_fn(&n, "Val", sort(ty))
    }

    fn vfield_fn(&mut self, ename: &str, vname: &str, field: &str, ty: &Ty) -> String {
        let n = if ename == "Option" {
            format!("opt_value_{}", sort(ty))
        } else {
            format!("vf_{}_{}_{}", ename, vname, field)
        };
        self.global_fn(&n, "Val", sort(ty))
    }

    fn obligation(&mut self, st: &St, pos: Pos, what: String, goal: String) {
        if st.dead {
            return;
        }
        let mut show: Vec<(String, String)> = Vec::new();
        let mut names: Vec<&String> = st.env.keys().collect();
        names.sort();
        for n in names {
            let v = &st.env[n];
            match &v.ty {
                Ty::Int | Ty::Bool => show.push((n.clone(), v.term.clone())),
                Ty::List(_) | Ty::Str | Ty::Map(..) | Ty::HashMap(..) => show.push((format!("len({})", n), format!("(len {})", v.term))),
                _ => {}
            }
        }
        self.obs.push(Ob { pos, what, facts: st.facts.clone(), goal, show });
    }

    fn str_const(&mut self, s: &str) -> String {
        if let Some(c) = self.strs.get(s) {
            return c.clone();
        }
        let c = self.fresh("Val");
        let b = s.as_bytes();
        self.defs.push(format!("(assert (= (len {}) {}))", c, b.len()));
        for (i, x) in b.iter().enumerate().take(64) {
            self.defs.push(format!("(assert (= (byte {} {}) {}))", c, i, x));
        }
        self.strs.insert(s.to_string(), c.clone());
        c
    }

    // ---- functions ----

    pub fn function(&mut self, f: &FnDecl) -> FnVc {
        self.n = 0;
        self.decls.clear();
        self.defs.clear();
        self.qdefs.clear();
        self.obs.clear();
        self.strs.clear();
        self.loops.clear();
        let fi = self.info.fns[&f.name].clone();
        self.fname = f.name.clone();
        self.ret = fi.ret.clone();
        self.ensures = fi.ensures.clone();
        self.decreases = fi.decreases.clone();
        self.params.clear();
        let mut st = St { env: HashMap::new(), facts: Vec::new(), src: Vec::new(), dead: false };
        for (pname, r) in &fi.params {
            let v = self.fresh(sort(&r.ty));
            self.params.insert(pname.clone(), v.clone());
            for p in &r.preds {
                let t = self.spec_term(p, &st, Some(v.clone()), None);
                st.facts.push(t);
                st.src.push(format!("{} where {}", pname, pretty::expr(p)));
            }
            let preds = if fi.inout[st.env.len()] { r.preds.clone() } else { Vec::new() };
            st.env.insert(pname.clone(), Var { term: v, ty: r.ty.clone(), preds });
        }
        for r in &fi.requires {
            let t = self.spec_term(r, &st, None, None);
            st.facts.push(t);
            st.src.push(pretty::expr(r));
        }
        self.block(&f.body, &mut st);
        if !st.dead && fi.ret.ty == Ty::Unit {
            self.check_post(&st, f.pos, "0".into());
        }
        FnVc {
            name: f.name.clone(),
            decls: self.decls.clone(),
            defs: self.defs.clone(),
            qdefs: self.qdefs.clone(),
            obs: self.obs.clone(),
        }
    }

    fn check_post(&mut self, st: &St, pos: Pos, val: String) {
        let ret = self.ret.clone();
        for p in &ret.preds {
            let g = self.spec_term(p, st, Some(val.clone()), None);
            self.obligation(st, pos, format!("return value of '{}' satisfies {}", self.fname, pretty::expr(p)), g);
        }
        let ens = self.ensures.clone();
        for e in &ens {
            let g = self.spec_term(e, st, None, Some(val.clone()));
            self.obligation(st, pos, format!("postcondition of '{}': {}", self.fname, pretty::expr(e)), g);
        }
    }

    // ---- specification terms ----

    /// Translate a specification predicate. Names resolve through the current
    /// environment; `it` and `result` are given explicitly.
    fn spec_term(&mut self, e: &Expr, st: &St, it: Option<String>, result: Option<String>) -> String {
        let mut binds = HashMap::new();
        for (k, v) in &st.env {
            binds.insert(k.clone(), v.term.clone());
        }
        // Parameters keep their entry values in postconditions.
        for (k, v) in &self.params {
            binds.entry(k.clone()).or_insert(v.clone());
        }
        let sp = Spec { binds, it, result };
        self.sexpr(e, &sp)
    }

    fn spec_with(&mut self, e: &Expr, binds: HashMap<String, String>, it: Option<String>) -> String {
        let sp = Spec { binds, it, result: None };
        self.sexpr(e, &sp)
    }

    fn sexpr(&mut self, e: &Expr, sp: &Spec) -> String {
        match &e.kind {
            EK::Int(n) => int_lit(*n),
            EK::Bool(b) => b.to_string(),
            EK::Str(s) => self.str_const(s),
            EK::Var(n) => sp.binds.get(n).cloned().unwrap_or_else(|| panic!("unbound '{}' in spec", n)),
            EK::It => sp.it.clone().expect("it"),
            EK::Result => sp.result.clone().expect("result"),
            EK::Call(_, args) => {
                let ci = self.info.calls[&e.id].clone();
                let mut p = Vec::new();
                for i in &ci.order {
                    p.push(self.sexpr(&args[*i].value, sp));
                }
                match &ci.kind {
                    CallKind::Builtin(n) => match n.as_str() {
                        "len" => format!("(len {})", p[0]),
                        "is_some" => format!("(= (tag {}) 1)", p[0]),
                        "is_none" => format!("(= (tag {}) 0)", p[0]),
                        _ => {
                            let t = self.info.ty(e);
                            let f = self.vfield_fn("Option", "Some", "value", &t);
                            format!("(ite (= (tag {o}) 1) ({f} {o}) {d})", o = p[0], f = f, d = p[1])
                        }
                    },
                    CallKind::User(n) => self.inline_call(n, &p),
                    _ => panic!("constructor in spec"),
                }
            }
            EK::Quant(q, v, lo, hi, body) => {
                let a = self.sexpr(lo, sp);
                let b = self.sexpr(hi, sp);
                self.n += 1;
                let qv = format!("q{}", self.n);
                let mut binds = sp.binds.clone();
                binds.insert(v.clone(), qv.clone());
                let inner = Spec { binds, it: sp.it.clone(), result: sp.result.clone() };
                let body = self.sexpr(body, &inner);
                quant_term(q, &qv, &a, &b, &body)
            }
            EK::Field(o, f) => {
                let ot = self.info.ty(o);
                let ft = self.info.ty(e);
                let ov = self.sexpr(o, sp);
                match ot {
                    Ty::Struct(sn) => {
                        let fnn = self.field_fn(&sn, f, &ft);
                        format!("({} {})", fnn, ov)
                    }
                    _ => panic!("field of non-struct in spec"),
                }
            }
            EK::Index(o, i) => {
                let ot = self.info.ty(o);
                let ov = self.sexpr(o, sp);
                let iv = self.sexpr(i, sp);
                match ot {
                    Ty::Str => format!("(byte {} {})", ov, iv),
                    Ty::List(t) => format!("(elem_{} {} {})", sort(&t), ov, iv),
                    _ => panic!("index in spec"),
                }
            }
            EK::Unary(op, x) => {
                let v = self.sexpr(x, sp);
                if op == "-" {
                    format!("(- {})", v)
                } else {
                    format!("(not {})", v)
                }
            }
            EK::Binary(op, l, r) => {
                let a = self.sexpr(l, sp);
                let b = self.sexpr(r, sp);
                binop_term(op, &a, &b)
            }
            _ => panic!("unsupported spec expression"),
        }
    }

    /// The body of an expression function with its parameters bound to `args`.
    fn inline_call(&mut self, name: &str, args: &[String]) -> String {
        let fi = self.info.fns[name].clone();
        let body = fi.inline.clone().expect("only expression functions are inlined");
        let mut binds = HashMap::new();
        for (i, (pn, _)) in fi.params.iter().enumerate() {
            binds.insert(pn.clone(), args[i].clone());
        }
        self.sexpr(&body, &Spec { binds, it: None, result: None })
    }

    /// The SMT function giving the contents of a String or List.
    fn content_fn(t: &Ty) -> Option<String> {
        match t {
            Ty::Str => Some("byte".into()),
            Ty::List(e) => Some(format!("elem_{}", sort(e))),
            _ => None,
        }
    }

    /// True if `e` can be translated as a specification term (see check::spec_safe).
    fn translatable(&self, e: &Expr) -> bool {
        match &e.kind {
            EK::Int(_) | EK::Bool(_) | EK::Str(_) | EK::Var(_) => true,
            EK::Unary(_, x) => self.translatable(x),
            EK::Binary(op, l, r) => op != "++" && self.translatable(l) && self.translatable(r),
            EK::Index(o, i) => self.translatable(o) && self.translatable(i),
            EK::Field(o, _) => {
                matches!(self.info.ty(o), Ty::Struct(_)) && self.translatable(o)
            }
            EK::Call(_, args) => {
                let ok = match self.info.calls.get(&e.id).map(|c| &c.kind) {
                    Some(CallKind::Builtin(n)) => crate::check::SPEC_BUILTINS.contains(&n.as_str()),
                    Some(CallKind::User(n)) => self.info.fns[n].inline.is_some(),
                    _ => false,
                };
                ok && args.iter().all(|a| self.translatable(&a.value))
            }
            EK::Quant(_, _, lo, hi, b) => self.translatable(lo) && self.translatable(hi) && self.translatable(b),
            _ => false,
        }
    }

    fn fresh_q(&mut self) -> String {
        self.n += 1;
        format!("q{}", self.n)
    }

    // ---- statements ----

    fn block(&mut self, stmts: &[Stmt], st: &mut St) {
        for s in stmts {
            if st.dead {
                return;
            }
            self.stmt(s, st);
        }
    }

    fn assume_preds(&mut self, st: &mut St, name: &str, preds: &[Expr], term: &str) {
        for p in preds {
            let t = self.spec_term(p, st, Some(term.to_string()), None);
            st.facts.push(t);
            st.src.push(format!("{} where {}", name, pretty::expr(p)));
        }
    }

    fn check_preds(&mut self, st: &St, pos: Pos, name: &str, preds: &[Expr], term: &str) {
        for p in preds {
            let g = self.spec_term(p, st, Some(term.to_string()), None);
            self.obligation(st, pos, format!("'{}' satisfies {}", name, pretty::expr(p)), g);
        }
    }

    fn stmt(&mut self, s: &Stmt, st: &mut St) {
        match &s.kind {
            SK::Let { name, ty, value, .. } => {
                let v = self.expr(value, st);
                if name == "_" {
                    return;
                }
                let r = match ty {
                    Some(te) => self.info.resolve(te),
                    None => RTy::plain(self.info.ty(value)),
                };
                self.check_preds(st, s.pos, name, &r.preds, &v);
                st.env.insert(name.clone(), Var { term: v, ty: r.ty, preds: r.preds });
            }
            SK::Set { name, value } => {
                let v = self.expr(value, st);
                let mut var = st.env[name].clone();
                self.check_preds(st, s.pos, name, &var.preds, &v);
                var.term = v;
                st.env.insert(name.clone(), var);
            }
            SK::Return(e) => {
                let v = match e {
                    Some(e) => self.expr(e, st),
                    None => "0".into(),
                };
                self.check_post(st, s.pos, v);
                st.dead = true;
            }
            SK::If { arms, els } => {
                let base = st.clone();
                let mut cur = st.clone();
                let mut results: Vec<(String, St)> = Vec::new();
                let mut negs: Vec<String> = Vec::new();
                for (c, body) in arms {
                    let ct = self.expr(c, &mut cur);
                    let mut arm = cur.clone();
                    arm.facts.push(ct.clone());
                    arm.src.push(pretty::expr(c));
                    self.block(body, &mut arm);
                    let mut g = negs.clone();
                    g.push(ct.clone());
                    results.push((and_all(&g), arm));
                    cur.facts.push(format!("(not {})", ct));
                    cur.src.push(format!("not ({})", pretty::expr(c)));
                    negs.push(format!("(not {})", ct));
                }
                if let Some(b) = els {
                    self.block(b, &mut cur);
                }
                results.push((and_all(&negs), cur));
                *st = self.merge(&base, results);
            }
            SK::Match { scrut, cases, els } => {
                let sv = self.expr(scrut, st);
                let stype = self.info.ty(scrut);
                let base = st.clone();
                let variants = self.info.variants(&stype);
                let mut results: Vec<(String, St)> = Vec::new();
                let mut guards: Vec<String> = Vec::new();
                for c in cases {
                    let mut arm = st.clone();
                    let g = match &c.pat {
                        Pat::Int(n) => format!("(= {} {})", sv, int_lit(*n)),
                        Pat::Str(s) => {
                            let k = self.str_const(s);
                            format!("(= {} {})", sv, k)
                        }
                        Pat::Variant { name, binds } => {
                            let vs = variants.as_ref().unwrap();
                            let vi = vs.iter().position(|v| v.name == *name).unwrap();
                            let vinfo = vs[vi].clone();
                            let ename = match &stype {
                                Ty::Opt(_) => "Option".to_string(),
                                Ty::Enum(n) => n.clone(),
                                _ => unreachable!(),
                            };
                            let mut fbinds = HashMap::new();
                            for (fname, fr) in &vinfo.fields {
                                let fnn = self.vfield_fn(&ename, name, fname, &fr.ty);
                                fbinds.insert(fname.clone(), format!("({} {})", fnn, sv));
                            }
                            arm.src.push(format!("{} is {}", pretty::expr(scrut), name));
                            for (fname, fr) in &vinfo.fields {
                                let ft = fbinds[fname].clone();
                                for p in &fr.preds {
                                    let t = self.spec_with(p, fbinds.clone(), Some(ft.clone()));
                                    arm.facts.push(t);
                                }
                                if let Some((_, b)) = binds.iter().find(|(f, _)| f == fname) {
                                    arm.env.insert(b.clone(), Var { term: ft.clone(), ty: fr.ty.clone(), preds: Vec::new() });
                                }
                            }
                            format!("(= (tag {}) {})", sv, vi)
                        }
                    };
                    arm.facts.push(g.clone());
                    self.block(&c.body, &mut arm);
                    guards.push(g.clone());
                    results.push((g, arm));
                }
                let all_neg: Vec<String> = guards.iter().map(|g| format!("(not {})", g)).collect();
                match els {
                    Some(b) => {
                        let mut arm = st.clone();
                        arm.facts.extend(all_neg.iter().cloned());
                        self.block(b, &mut arm);
                        results.push((and_all(&all_neg), arm));
                    }
                    None => {
                        if variants.is_some() {
                            // Exhaustive: the tag is one of the variants.
                            let n = variants.as_ref().unwrap().len();
                            let range = format!("(and (<= 0 (tag {})) (< (tag {}) {}))", sv, sv, n);
                            for (_, r) in results.iter_mut() {
                                r.facts.push(range.clone());
                            }
                            let mut f = base.clone();
                            f.facts.push(range);
                            *st = self.merge(&f, results);
                            return;
                        }
                    }
                }
                *st = self.merge(&base, results);
            }
            SK::While { cond, invs, dec, body } => {
                for i in invs {
                    let g = self.spec_term(i, st, None, None);
                    self.obligation(st, i.pos, format!("loop invariant holds on entry: {}", pretty::expr(i)), g);
                }
                let mut head = self.havoc(body, st, invs);
                let c = self.expr(cond, &mut head);
                let mut inner = head.clone();
                inner.facts.push(c.clone());
                inner.src.push(pretty::expr(cond));
                let measure = match dec {
                    Some(d) => {
                        let t = self.spec_term(d, &inner, None, None);
                        let d0 = self.define("Int", t);
                        let what = format!("loop measure is non-negative: {}", pretty::expr(d));
                        self.obligation(&inner, d.pos, what, format!("(>= {} 0)", d0));
                        Some((d.clone(), d0))
                    }
                    None => None,
                };
                self.loops.push(Loop { invs: invs.clone(), next: None, dec: measure.clone() });
                self.block(body, &mut inner);
                self.loops.pop();
                self.check_invs(&inner, invs, "is preserved");
                self.check_measure(&inner, &measure);
                let mut after = head;
                if !has_break(body) {
                    after.facts.push(format!("(not {})", c));
                    after.src.push(format!("not ({})", pretty::expr(cond)));
                }
                *st = after;
            }
            SK::ForRange { var, lo, hi, invs, body } => {
                let a = self.expr(lo, st);
                let b = self.expr(hi, st);
                let mut entry = st.clone();
                entry.env.insert(var.clone(), Var { term: a.clone(), ty: Ty::Int, preds: Vec::new() });
                for i in invs {
                    let g = self.spec_term(i, &entry, None, None);
                    self.obligation(&entry, i.pos, format!("loop invariant holds on entry: {}", pretty::expr(i)), g);
                }
                // Verified like the counted while loop it is: invariants may mention the
                // loop variable, which is lo on entry and one more after each iteration.
                let mut head = self.havoc(body, st, &[]);
                let iv = self.fresh("Int");
                head.facts.push(format!("(<= {} {})", a, iv));
                head.facts.push(format!("(or (<= {} {}) (= {} {}))", iv, b, iv, a));
                head.env.insert(var.clone(), Var { term: iv.clone(), ty: Ty::Int, preds: Vec::new() });
                for i in invs {
                    let t = self.spec_term(i, &head, None, None);
                    head.facts.push(t);
                    head.src.push(pretty::expr(i));
                }
                let mut inner = head.clone();
                inner.facts.push(format!("(< {} {})", iv, b));
                inner.src.push(format!("{} <= {}", pretty::expr(lo), var));
                inner.src.push(format!("{} < {}", var, pretty::expr(hi)));
                let next = self.define("Int", format!("(+ {} 1)", iv));
                self.loops.push(Loop { invs: invs.clone(), next: Some((var.clone(), next.clone())), dec: None });
                self.block(body, &mut inner);
                self.loops.pop();
                let mut end = inner.clone();
                end.env.get_mut(var).unwrap().term = next;
                self.check_invs(&end, invs, "is preserved");
                let mut after = head;
                if !has_break(body) {
                    after.facts.push(format!("(<= {} {})", b, iv));
                }
                after.env.remove(var);
                *st = after;
            }
            SK::ForEach { var, list, invs, body } => {
                let l = self.expr(list, st);
                let et = match self.info.ty(list) {
                    Ty::List(t) => *t,
                    _ => Ty::Unknown,
                };
                for i in invs {
                    let g = self.spec_term(i, st, None, None);
                    self.obligation(st, i.pos, format!("loop invariant holds on entry: {}", pretty::expr(i)), g);
                }
                let head = self.havoc(body, st, invs);
                let mut inner = head.clone();
                let k = self.fresh("Int");
                inner.facts.push(format!("(<= 0 {})", k));
                inner.facts.push(format!("(< {} (len {}))", k, l));
                let x = self.define(sort(&et), format!("(elem_{} {} {})", sort(&et), l, k));
                inner.env.insert(var.clone(), Var { term: x, ty: et, preds: Vec::new() });
                inner.src.push(format!("{} is an element of {}", var, pretty::expr(list)));
                self.loops.push(Loop { invs: invs.clone(), next: None, dec: None });
                self.block(body, &mut inner);
                self.loops.pop();
                self.check_invs(&inner, invs, "is preserved");
                *st = head;
            }
            SK::Assert(e) => {
                let g = self.spec_term(e, st, None, None);
                self.obligation(st, s.pos, format!("assertion: {}", pretty::expr(e)), g.clone());
                st.facts.push(g);
                st.src.push(pretty::expr(e));
            }
            SK::Check(e) => {
                let g = self.expr(e, st);
                st.facts.push(g);
                st.src.push(pretty::expr(e));
            }
            SK::Break | SK::Continue => {
                let invs = self.loops.last().map(|l| l.invs.clone()).unwrap_or_default();
                let what = if matches!(s.kind, SK::Break) { "holds at 'break'" } else { "holds at 'continue'" };
                let mut at = st.clone();
                if matches!(s.kind, SK::Continue) {
                    if let Some((v, next)) = self.loops.last().and_then(|l| l.next.clone()) {
                        at.env.get_mut(&v).unwrap().term = next;
                    }
                }
                self.check_invs(&at, &invs, what);
                if matches!(s.kind, SK::Continue) {
                    let measure = self.loops.last().and_then(|l| l.dec.clone());
                    self.check_measure(st, &measure);
                }
                st.dead = true;
            }
            SK::Expr(e) => {
                self.expr(e, st);
                if is_noreturn(e) {
                    st.dead = true;
                }
            }
        }
    }

    fn check_measure(&mut self, st: &St, measure: &Option<(Expr, String)>) {
        if let Some((d, d0)) = measure {
            let now = self.spec_term(d, st, None, None);
            let what = format!("loop measure decreases: {}", pretty::expr(d));
            self.obligation(st, d.pos, what, format!("(< {} {})", now, d0));
        }
    }

    fn check_invs(&mut self, st: &St, invs: &[Expr], what: &str) {
        for i in invs {
            let g = self.spec_term(i, st, None, None);
            self.obligation(st, i.pos, format!("loop invariant {}: {}", what, pretty::expr(i)), g);
        }
    }

    /// The state at a loop head: variables assigned in the body get fresh values
    /// constrained only by their declared refinements and the loop invariants.
    fn havoc(&mut self, body: &[Stmt], st: &St, invs: &[Expr]) -> St {
        let mut names = Vec::new();
        assigned(body, &mut names);
        let mut h = st.clone();
        for n in &names {
            if let Some(var) = st.env.get(n).cloned() {
                let v = self.fresh(sort(&var.ty));
                let preds = var.preds.clone();
                h.env.insert(n.clone(), Var { term: v.clone(), ty: var.ty.clone(), preds: var.preds.clone() });
                self.assume_preds(&mut h, n, &preds, &v);
            }
        }
        // Facts mentioning the old values of havocked variables stay true of those old
        // terms, so they can be kept; human-readable facts may now be stale, though.
        h.src.retain(|f| !names.iter().any(|n| mentions(f, n)));
        for i in invs {
            let t = self.spec_term(i, &h, None, None);
            h.facts.push(t);
            h.src.push(pretty::expr(i));
        }
        h
    }

    fn merge(&mut self, base: &St, results: Vec<(String, St)>) -> St {
        let live: Vec<(String, St)> = results.into_iter().filter(|(_, s)| !s.dead).collect();
        let mut out = base.clone();
        if live.is_empty() {
            out.dead = true;
            return out;
        }
        let k = base.facts.len();
        let guards: Vec<String> = live.iter().map(|(g, _)| g.clone()).collect();
        if live.len() < 2 || guards.len() > 0 {
            out.facts.push(if guards.len() == 1 { guards[0].clone() } else { format!("(or {})", guards.join(" ")) });
        }
        for (g, s) in &live {
            let new: Vec<String> = s.facts[k..].to_vec();
            if !new.is_empty() {
                out.facts.push(format!("(=> {} {})", g, and_all(&new)));
            }
        }
        if live.len() == 1 {
            out.src = live[0].1.src.clone();
        }
        let mut names: Vec<String> = base.env.keys().cloned().collect();
        names.sort();
        for n in names {
            let terms: Vec<String> = live.iter().map(|(_, s)| s.env[&n].term.clone()).collect();
            if terms.iter().all(|t| *t == terms[0]) {
                let mut v = base.env[&n].clone();
                v.term = terms[0].clone();
                out.env.insert(n, v);
                continue;
            }
            let mut t = terms[terms.len() - 1].clone();
            for i in (0..terms.len() - 1).rev() {
                t = format!("(ite {} {} {})", live[i].0, terms[i], t);
            }
            let ty = base.env[&n].ty.clone();
            let v = self.define(sort(&ty), t);
            out.env.get_mut(&n).unwrap().term = v;
        }
        out
    }

    // ---- expressions ----

    fn expr(&mut self, e: &Expr, st: &mut St) -> String {
        match &e.kind {
            EK::Int(n) => int_lit(*n),
            EK::Bool(b) => b.to_string(),
            EK::Str(s) => self.str_const(s),
            EK::Var(n) => st.env[n].term.clone(),
            EK::It | EK::Result => panic!("it/result outside a specification"),
            EK::Hole(_) => {
                let mut facts = st.src.clone();
                facts.dedup();
                self.hole_facts.insert(e.id, facts);
                let t = self.info.ty(e);
                self.fresh(sort(&t))
            }
            EK::Call(_, args) => self.call(e, args, st),
            EK::List(items) => {
                let mut vals = Vec::new();
                for i in items {
                    vals.push(self.expr(i, st));
                }
                let et = match self.info.ty(e) {
                    Ty::List(t) => *t,
                    _ => Ty::Unknown,
                };
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (len {}) {}))", r, vals.len()));
                for (i, v) in vals.iter().enumerate() {
                    self.defs.push(format!("(assert (= (elem_{} {} {}) {}))", sort(&et), r, i, v));
                }
                r
            }
            EK::EmptyMap => {
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (len {}) 0))", r));
                r
            }
            EK::Index(o, i) => {
                let ov = self.expr(o, st);
                let iv = self.expr(i, st);
                let goal = format!("(and (<= 0 {}) (< {} (len {})))", iv, iv, ov);
                let what = format!("index in bounds: 0 <= {} < len({})", pretty::expr(i), pretty::expr(o));
                self.obligation(st, e.pos, what, goal);
                match self.info.ty(o) {
                    Ty::Str => format!("(byte {} {})", ov, iv),
                    Ty::List(t) => format!("(elem_{} {} {})", sort(&t), ov, iv),
                    _ => self.fresh("Val"),
                }
            }
            EK::Field(o, f) => {
                if let Some(ci) = self.info.calls.get(&e.id) {
                    if let CallKind::Variant(_, vi) = ci.kind {
                        let r = self.fresh("Val");
                        self.defs.push(format!("(assert (= (tag {}) {}))", r, vi));
                        return r;
                    }
                }
                let ov = self.expr(o, st);
                let sn = match self.info.ty(o) {
                    Ty::Struct(n) => n,
                    _ => return self.fresh("Val"),
                };
                let fields = self.info.structs[&sn].clone();
                let mut binds = HashMap::new();
                for (fname, fr) in &fields {
                    let fnn = self.field_fn(&sn, fname, &fr.ty);
                    binds.insert(fname.clone(), format!("({} {})", fnn, ov));
                }
                let (_, fr) = fields.iter().find(|(n, _)| n == f).unwrap().clone();
                let term = binds[f].clone();
                for p in &fr.preds {
                    let t = self.spec_with(p, binds.clone(), Some(term.clone()));
                    st.facts.push(t);
                }
                term
            }
            EK::With(o, args) => {
                let ov = self.expr(o, st);
                // The original value satisfies its struct's field refinements, so the
                // fields that 'with' leaves unchanged still satisfy them.
                if let Ty::Struct(sn) = self.info.ty(o) {
                    let fields = self.info.structs[&sn].clone();
                    let mut binds = HashMap::new();
                    for (fname, fr) in &fields {
                        let fnn = self.field_fn(&sn, fname, &fr.ty);
                        binds.insert(fname.clone(), format!("({} {})", fnn, ov));
                    }
                    for (fname, fr) in &fields {
                        for p in &fr.preds {
                            let t = self.spec_with(p, binds.clone(), Some(binds[fname].clone()));
                            st.facts.push(t);
                        }
                    }
                }
                let mut vals: HashMap<String, String> = HashMap::new();
                for a in args {
                    let v = self.expr(&a.value, st);
                    vals.insert(a.name.clone().unwrap(), v);
                }
                let sn = match self.info.ty(o) {
                    Ty::Struct(n) => n,
                    _ => return self.fresh("Val"),
                };
                self.construct_struct(st, e.pos, &sn, |me, fname, fty| match vals.get(fname) {
                    Some(v) => v.clone(),
                    None => {
                        let fnn = me.field_fn(&sn, fname, fty);
                        format!("({} {})", fnn, ov)
                    }
                })
            }
            EK::Unary(op, x) => {
                let v = self.expr(x, st);
                if op == "-" {
                    format!("(- {})", v)
                } else {
                    format!("(not {})", v)
                }
            }
            EK::Quant(q, v, lo, hi, body) => {
                let a = self.expr(lo, st);
                let b = self.expr(hi, st);
                // Obligations of the body, for every value of the bound variable.
                let mut sub = st.clone();
                let iv = self.fresh("Int");
                sub.facts.push(format!("(<= {} {})", a, iv));
                sub.facts.push(format!("(< {} {})", iv, b));
                sub.env.insert(v.clone(), Var { term: iv, ty: Ty::Int, preds: Vec::new() });
                self.expr(body, &mut sub);
                if !self.translatable(body) {
                    return self.fresh("Bool");
                }
                // The value: the quantified formula over the current environment.
                let mut binds = HashMap::new();
                for (k, x) in &st.env {
                    binds.insert(k.clone(), x.term.clone());
                }
                let qv = self.fresh_q();
                binds.insert(v.clone(), qv.clone());
                let bt = self.sexpr(body, &Spec { binds, it: None, result: None });
                self.define("Bool", quant_term(q, &qv, &a, &b, &bt))
            }
            EK::Binary(op, l, r) => {
                if op == "and" || op == "or" || op == "implies" {
                    let a = self.expr(l, st);
                    let guard = if op == "or" { format!("(not {})", a) } else { a.clone() };
                    let mut sub = st.clone();
                    sub.facts.push(guard.clone());
                    let b = self.expr(r, &mut sub);
                    let k = st.facts.len() + 1;
                    let new: Vec<String> = sub.facts[k..].to_vec();
                    if !new.is_empty() {
                        st.facts.push(format!("(=> {} {})", guard, and_all(&new)));
                    }
                    return self.define("Bool", binop_term(op, &a, &b));
                }
                let a = self.expr(l, st);
                let b = self.expr(r, st);
                match op.as_str() {
                    "/" | "%" => {
                        self.obligation(st, e.pos, format!("divisor is not zero: {}", pretty::expr(r)), format!("(not (= {} 0))", b));
                        self.define("Int", binop_term(op, &a, &b))
                    }
                    "++" => {
                        let r = self.fresh("Val");
                        self.defs.push(format!("(assert (= (len {}) (+ (len {}) (len {}))))", r, a, b));
                        // Triggered only when the result's contents are inspected.
                        if let Some(f) = Self::content_fn(&self.info.ty(l)) {
                            let q = self.fresh_q();
                            self.qdefs.push(format!(
                                "(assert (forall (({q} Int)) (! (=> (and (<= 0 {q}) (< {q} (len {r}))) (= ({f} {r} {q}) (ite (< {q} (len {a})) ({f} {a} {q}) ({f} {b} (- {q} (len {a})))))) :pattern (({f} {r} {q})))))",
                                q = q, a = a, b = b, f = f, r = r
                            ));
                        }
                        r
                    }
                    "==" | "!=" | "<" | "<=" | ">" | ">=" => self.define("Bool", binop_term(op, &a, &b)),
                    _ => self.define("Int", binop_term(op, &a, &b)),
                }
            }
        }
    }

    fn construct_struct<F>(&mut self, st: &mut St, pos: Pos, sn: &str, mut value: F) -> String
    where
        F: FnMut(&mut Self, &str, &Ty) -> String,
    {
        let fields = self.info.structs[sn].clone();
        let mut binds = HashMap::new();
        for (fname, fr) in &fields {
            let v = value(self, fname, &fr.ty);
            binds.insert(fname.clone(), v);
        }
        for (fname, fr) in &fields {
            for p in &fr.preds {
                let g = self.spec_with(p, binds.clone(), Some(binds[fname].clone()));
                self.obligation(st, pos, format!("field '{}' of {} satisfies {}", fname, sn, pretty::expr(p)), g);
            }
        }
        let r = self.fresh("Val");
        for (fname, fr) in &fields {
            let fnn = self.field_fn(sn, fname, &fr.ty);
            self.defs.push(format!("(assert (= ({} {}) {}))", fnn, r, binds[fname]));
        }
        r
    }

    fn call(&mut self, e: &Expr, args: &[Arg], st: &mut St) -> String {
        let ci = self.info.calls[&e.id].clone();
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(&a.value, st));
        }
        let p: Vec<String> = ci.order.iter().map(|i| vals[*i].clone()).collect();
        let pe: Vec<&Expr> = ci.order.iter().map(|i| &args[*i].value).collect();
        match &ci.kind {
            CallKind::User(name) => {
                let fi = self.info.fns[name].clone();
                let mut binds: HashMap<String, String> = HashMap::new();
                for (i, (pname, r)) in fi.params.iter().enumerate() {
                    for pr in &r.preds {
                        let g = self.spec_with(pr, binds.clone(), Some(p[i].clone()));
                        let what = format!("argument '{}' of '{}' satisfies {}", pname, name, pretty::expr(pr));
                        self.obligation(st, pe[i].pos, what, g);
                    }
                    binds.insert(pname.clone(), p[i].clone());
                }
                for r in &fi.requires {
                    let g = self.spec_with(r, binds.clone(), None);
                    self.obligation(st, e.pos, format!("precondition of '{}': {}", name, pretty::expr(r)), g);
                }
                // Measured on the arguments as passed in.
                if name == &self.fname {
                    if let Some(d) = self.decreases.clone() {
                        let entry = self.spec_with(&d, self.params.clone(), None);
                        let now = self.spec_with(&d, binds.clone(), None);
                        let what = format!("recursive call to '{}' decreases its measure: {}", name, pretty::expr(&d));
                        self.obligation(st, e.pos, what, format!("(and (>= {} 0) (< {} {}))", entry, now, entry));
                    }
                }
                // Each inout argument's var receives a new value.
                let mut post = binds.clone();
                let mut updated: Vec<(String, String)> = Vec::new();
                for (i, (pname, r)) in fi.params.iter().enumerate() {
                    if fi.inout[i] {
                        let nv = self.fresh(sort(&r.ty));
                        post.insert(pname.clone(), nv.clone());
                        if let EK::Var(vn) = &pe[i].kind {
                            updated.push((vn.clone(), nv));
                        }
                    }
                }
                for (i, (_, r)) in fi.params.iter().enumerate() {
                    if fi.inout[i] {
                        let nv = post[&fi.params[i].0].clone();
                        for pr in &r.preds {
                            let t = self.spec_with(pr, post.clone(), Some(nv.clone()));
                            st.facts.push(t);
                        }
                    }
                }
                let binds = post;
                let res = self.fresh(sort(&fi.ret.ty));
                if fi.inline.is_some() {
                    // Expression functions are transparent: the result is the body.
                    let t = self.inline_call(name, &p);
                    st.facts.push(format!("(= {} {})", res, t));
                }
                for pr in &fi.ret.preds {
                    let t = self.spec_with(pr, binds.clone(), Some(res.clone()));
                    st.facts.push(t);
                }
                for en in &fi.ensures {
                    let sp = Spec { binds: binds.clone(), it: None, result: Some(res.clone()) };
                    let t = self.sexpr(en, &sp);
                    st.facts.push(t);
                }
                for (vn, nv) in updated {
                    let mut var = st.env[&vn].clone();
                    let preds = var.preds.clone();
                    self.check_preds(st, e.pos, &vn, &preds, &nv);
                    var.term = nv;
                    st.env.insert(vn, var);
                }
                res
            }
            CallKind::Struct(sn) => {
                let fields = self.info.structs[sn].clone();
                let byname: HashMap<String, String> =
                    fields.iter().enumerate().map(|(i, (n, _))| (n.clone(), p[i].clone())).collect();
                self.construct_struct(st, e.pos, sn, |_, fname, _| byname[fname].clone())
            }
            CallKind::Variant(ename, vi) => {
                let ty = self.info.ty(e);
                let vinfo = self.info.variants(&ty).unwrap()[*vi].clone();
                let mut binds = HashMap::new();
                for (i, (fname, _)) in vinfo.fields.iter().enumerate() {
                    binds.insert(fname.clone(), p[i].clone());
                }
                for (fname, fr) in &vinfo.fields {
                    for pr in &fr.preds {
                        let g = self.spec_with(pr, binds.clone(), Some(binds[fname].clone()));
                        let what = format!("field '{}' of {}.{} satisfies {}", fname, ename, vinfo.name, pretty::expr(pr));
                        self.obligation(st, e.pos, what, g);
                    }
                }
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (tag {}) {}))", r, vi));
                for (fname, fr) in &vinfo.fields {
                    let fnn = self.vfield_fn(ename, &vinfo.name, fname, &fr.ty);
                    self.defs.push(format!("(assert (= ({} {}) {}))", fnn, r, binds[fname]));
                }
                r
            }
            CallKind::Builtin(name) => self.builtin(e, name, &p, &pe, st),
        }
    }

    fn builtin(&mut self, e: &Expr, name: &str, p: &[String], pe: &[&Expr], st: &mut St) -> String {
        let rty = self.info.ty(e);
        match name {
            "len" => self.define("Int", format!("(len {})", p[0])),
            "push" => {
                let et = match &rty {
                    Ty::List(t) => (**t).clone(),
                    _ => Ty::Unknown,
                };
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (len {}) (+ (len {}) 1)))", r, p[0]));
                self.defs.push(format!("(assert (= (elem_{} {} (len {})) {}))", sort(&et), r, p[0], p[1]));
                let q = self.fresh_q();
                self.qdefs.push(format!(
                    "(assert (forall (({q} Int)) (! (=> (and (<= 0 {q}) (< {q} (len {a}))) (= (elem_{s} {r} {q}) (elem_{s} {a} {q}))) :pattern ((elem_{s} {r} {q})))))",
                    q = q, a = p[0], s = sort(&et), r = r
                ));
                r
            }
            "set_at" => {
                let goal = format!("(and (<= 0 {i}) (< {i} (len {l})))", i = p[1], l = p[0]);
                let what = format!("set_at index in bounds: 0 <= {} < len({})", pretty::expr(pe[1]), pretty::expr(pe[0]));
                self.obligation(st, e.pos, what, goal);
                let et = match &rty {
                    Ty::List(t) => (**t).clone(),
                    _ => Ty::Unknown,
                };
                let s = sort(&et);
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (len {}) (len {})))", r, p[0]));
                self.defs.push(format!("(assert (= (elem_{} {} {}) {}))", s, r, p[1], p[2]));
                let q = self.fresh_q();
                self.qdefs.push(format!(
                    "(assert (forall (({q} Int)) (! (=> (and (<= 0 {q}) (< {q} (len {l})) (not (= {q} {i}))) (= (elem_{s} {r} {q}) (elem_{s} {l} {q}))) :pattern ((elem_{s} {r} {q})))))",
                    q = q, l = p[0], i = p[1], s = s, r = r
                ));
                r
            }
            "slice" => {
                let goal = format!("(and (<= 0 {f}) (<= {f} {t}) (<= {t} (len {o})))", f = p[1], t = p[2], o = p[0]);
                let what = format!(
                    "slice bounds: 0 <= {} <= {} <= len({})",
                    pretty::expr(pe[1]),
                    pretty::expr(pe[2]),
                    pretty::expr(pe[0])
                );
                self.obligation(st, e.pos, what, goal);
                // Only meaningful once the bounds hold, so it is a path fact, not a
                // global definition: len(r) = to - from would otherwise force to >= from
                // through the len >= 0 axiom and make the obligation vacuous.
                let r = self.fresh("Val");
                st.facts.push(format!("(= (len {}) (- {} {}))", r, p[2], p[1]));
                if let Some(f) = Self::content_fn(&rty) {
                    let q = self.fresh_q();
                    st.facts.push(format!(
                        "(forall (({q} Int)) (! (=> (and (<= 0 {q}) (< {q} (- {t} {fr}))) (= ({f} {r} {q}) ({f} {o} (+ {fr} {q})))) :pattern (({f} {r} {q}))))",
                        q = q, t = p[2], fr = p[1], f = f, r = r, o = p[0]
                    ));
                }
                r
            }
            "char" => {
                let goal = format!("(and (<= 0 {c}) (< {c} 256))", c = p[0]);
                self.obligation(st, e.pos, format!("char code in range: 0 <= {} < 256", pretty::expr(pe[0])), goal);
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (len {}) 1))", r));
                st.facts.push(format!("(= (byte {} 0) {})", r, p[0]));
                r
            }
            "int_to_str" => {
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (>= (len {}) 1))", r));
                r
            }
            "hash" => {
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (len {}) 16))", r));
                r
            }
            "keys" => {
                let r = self.fresh("Val");
                self.defs.push(format!("(assert (= (len {}) (len {})))", r, p[0]));
                r
            }
            "panic" | "exit" | "print" | "eprint" => "0".into(),
            "is_some" => self.define("Bool", format!("(= (tag {}) 1)", p[0])),
            "is_none" => self.define("Bool", format!("(= (tag {}) 0)", p[0])),
            "value_or" => {
                let f = self.vfield_fn("Option", "Some", "value", &rty);
                self.define(sort(&rty), format!("(ite (= (tag {o}) 1) ({f} {o}) {d})", o = p[0], f = f, d = p[1]))
            }
            _ => self.fresh(sort(&rty)),
        }
    }
}

fn quant_term(q: &str, v: &str, lo: &str, hi: &str, body: &str) -> String {
    if q == "forall" {
        format!("(forall (({v} Int)) (=> (and (<= {lo} {v}) (< {v} {hi})) {body}))", v = v, lo = lo, hi = hi, body = body)
    } else {
        format!("(exists (({v} Int)) (and (<= {lo} {v}) (< {v} {hi}) {body}))", v = v, lo = lo, hi = hi, body = body)
    }
}

fn binop_term(op: &str, a: &str, b: &str) -> String {
    let o = match op {
        "+" => "+",
        "-" => "-",
        "*" => "*",
        "/" => "div",
        "%" => "mod",
        "<" => "<",
        "<=" => "<=",
        ">" => ">",
        ">=" => ">=",
        "==" => "=",
        "and" => "and",
        "or" => "or",
        "implies" => "=>",
        "!=" => return format!("(not (= {} {}))", a, b),
        _ => panic!("operator {}", op),
    };
    format!("({} {} {})", o, a, b)
}

fn is_noreturn(e: &Expr) -> bool {
    if let EK::Call(c, _) = &e.kind {
        if let EK::Var(n) = &c.kind {
            return builtin(n).map(|b| b.noreturn).unwrap_or(false);
        }
    }
    false
}

fn has_break(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        SK::Break => true,
        SK::If { arms, els } => arms.iter().any(|(_, b)| has_break(b)) || els.as_ref().map_or(false, |b| has_break(b)),
        SK::Match { cases, els, .. } => {
            cases.iter().any(|c| has_break(&c.body)) || els.as_ref().map_or(false, |b| has_break(b))
        }
        _ => false,
    })
}

fn mentions(text: &str, name: &str) -> bool {
    let b = text.as_bytes();
    let n = name.len();
    let mut i = 0;
    while let Some(off) = text[i..].find(name) {
        let s = i + off;
        let before = s == 0 || !(b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_');
        let after = s + n >= b.len() || !(b[s + n].is_ascii_alphanumeric() || b[s + n] == b'_');
        if before && after {
            return true;
        }
        i = s + 1;
    }
    false
}

// ---- running Z3 ----

pub struct Failure {
    pub fn_name: String,
    pub ob: Ob,
    pub verdict: String,
    pub model: Vec<(String, String)>,
}

fn run_z3(script: &str) -> Result<String, String> {
    use std::io::Write;
    let z3 = std::env::var("VOW_Z3").unwrap_or_else(|_| "z3".to_string());
    let mut child = std::process::Command::new(&z3)
        .args(["-in", "-smt2"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run Z3 ('{}'): {}; install it or set VOW_Z3", z3, e))?;
    let mut stdin = child.stdin.take().unwrap();
    let s = script.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(s.as_bytes());
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn script_for(pre: &[String], vcs: &[&FnVc], model: Option<&Ob>) -> String {
    let mut s = String::new();
    s.push_str("(set-option :produce-models true)\n(set-option :timeout 10000)\n");
    for p in pre {
        s.push_str(p);
        s.push('\n');
    }
    for vc in vcs {
        s.push_str(&format!("(push 1) ; fn {}\n", vc.name));
        for d in &vc.decls {
            s.push_str(d);
            s.push('\n');
        }
        for d in &vc.defs {
            s.push_str(d);
            s.push('\n');
        }
        if model.is_none() {
            // Sanity check: the definitions alone must be satisfiable, or every
            // obligation in this function would be proved vacuously.
            s.push_str("(check-sat)\n");
        }
        for d in &vc.qdefs {
            s.push_str(d);
            s.push('\n');
        }
        let obs: Vec<&Ob> = match model {
            Some(o) => vec![o],
            None => vc.obs.iter().collect(),
        };
        for ob in obs {
            s.push_str("(push 1)\n");
            for f in &ob.facts {
                s.push_str(&format!("(assert {})\n", f));
            }
            s.push_str(&format!("(assert (not {}))\n(check-sat)\n", ob.goal));
            if model.is_some() && !ob.show.is_empty() {
                let terms: Vec<String> = ob.show.iter().map(|(_, t)| t.clone()).collect();
                s.push_str(&format!("(get-value ({}))\n", terms.join(" ")));
            }
            s.push_str("(pop 1)\n");
        }
        s.push_str("(pop 1)\n");
    }
    s
}

/// Parse the values printed by (get-value ...), in order.
fn parse_values(out: &str) -> Vec<String> {
    // Output looks like: sat\n((v1 3)\n (v2 (- 1)))
    let start = match out.find("((") {
        Some(i) => i + 1,
        None => return Vec::new(),
    };
    let b = out.as_bytes();
    let mut vals = Vec::new();
    let mut i = start;
    while i < b.len() {
        if b[i] == b'(' {
            // one (term value) pair
            let mut depth = 0;
            let mut j = i;
            loop {
                if j >= b.len() {
                    return vals;
                }
                if b[j] == b'(' {
                    depth += 1;
                } else if b[j] == b')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                j += 1;
            }
            let pair = &out[i + 1..j];
            vals.push(split_pair(pair));
            i = j + 1;
        } else if b[i] == b')' {
            break;
        } else {
            i += 1;
        }
    }
    vals
}

fn split_pair(pair: &str) -> String {
    // "term value": the term is either an atom or a parenthesised expression.
    let p = pair.trim();
    let b = p.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'(') {
        let mut d = 0;
        while i < b.len() {
            if b[i] == b'(' {
                d += 1;
            } else if b[i] == b')' {
                d -= 1;
                if d == 0 {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }
    } else {
        while i < b.len() && !b[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    let v = p[i..].trim();
    // Render (- 5) as -5.
    if v.starts_with("(- ") && v.ends_with(')') {
        return format!("-{}", v[3..v.len() - 1].trim());
    }
    v.to_string()
}

pub fn discharge(pre: &[String], vcs: &[FnVc]) -> Result<Vec<Failure>, String> {
    let refs: Vec<&FnVc> = vcs.iter().collect();
    let script = script_for(pre, &refs, None);
    if let Ok(path) = std::env::var("VOW_SMT_DUMP") {
        let _ = std::fs::write(path, &script);
    }
    let out = run_z3(&script)?;
    let answers: Vec<&str> = out
        .lines()
        .map(|l| l.trim())
        .filter(|l| *l == "sat" || *l == "unsat" || *l == "unknown" || l.starts_with("(error"))
        .collect();
    if let Some(err) = answers.iter().find(|a| a.starts_with("(error")) {
        return Err(format!("Z3 rejected the generated query: {}", err));
    }
    let total: usize = vcs.iter().map(|v| v.obs.len() + 1).sum();
    if answers.len() != total {
        return Err(format!("Z3 answered {} of {} queries", answers.len(), total));
    }
    let mut failures = Vec::new();
    let mut k = 0;
    for vc in vcs {
        if answers[k] == "unsat" {
            return Err(format!("internal verifier error: the definitions for '{}' are inconsistent", vc.name));
        }
        k += 1;
        for ob in &vc.obs {
            let a = answers[k];
            k += 1;
            if a == "unsat" {
                continue;
            }
            let mut model = Vec::new();
            if a == "sat" && !ob.show.is_empty() {
                let out = run_z3(&script_for(pre, &[vc], Some(ob)))?;
                let vals = parse_values(&out);
                for (i, (name, _)) in ob.show.iter().enumerate() {
                    if let Some(v) = vals.get(i) {
                        model.push((name.clone(), v.clone()));
                    }
                }
            }
            failures.push(Failure { fn_name: vc.name.clone(), ob: ob.clone(), verdict: a.to_string(), model });
        }
    }
    Ok(failures)
}

pub fn failure_diag(f: &Failure) -> Diag {
    let mut d = Diag::new(f.ob.pos, &format!("cannot prove {}", f.ob.what));
    if f.verdict == "unknown" {
        d = d.note("Z3 gave up (timeout or incomplete theory); add an assertion, invariant or check to help it");
    } else if !f.model.is_empty() {
        let parts: Vec<String> = f.model.iter().map(|(n, v)| format!("{} = {}", n, v)).collect();
        d = d.note(&format!("counterexample: {}", parts.join(", ")));
    }
    d.note(&format!("in function '{}'", f.fn_name))
}
