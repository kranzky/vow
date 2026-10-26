use crate::ast::*;
use crate::diag::{Diag, Pos};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Bool,
    Str,
    Unit,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Opt(Box<Ty>),
    Struct(String),
    Enum(String),
    Unknown,
}

impl Ty {
    pub fn show(&self) -> String {
        match self {
            Ty::Int => "Int".into(),
            Ty::Bool => "Bool".into(),
            Ty::Str => "String".into(),
            Ty::Unit => "Unit".into(),
            Ty::List(t) => format!("List<{}>", t.show()),
            Ty::Map(k, v) => format!("Map<{}, {}>", k.show(), v.show()),
            Ty::Opt(t) => format!("Option<{}>", t.show()),
            Ty::Struct(n) | Ty::Enum(n) => n.clone(),
            Ty::Unknown => "?".into(),
        }
    }
}

/// A base type plus refinement predicates over `it`.
#[derive(Clone, Debug)]
pub struct RTy {
    pub ty: Ty,
    pub preds: Vec<Expr>,
}

impl RTy {
    pub fn plain(ty: Ty) -> RTy {
        RTy { ty, preds: Vec::new() }
    }
}

#[derive(Clone, Debug)]
pub struct VariantInfo {
    pub name: String,
    pub fields: Vec<(String, RTy)>,
}

#[derive(Clone, Debug)]
pub struct FnInfo {
    pub name: String,
    pub params: Vec<(String, RTy)>,
    pub ret: RTy,
    pub uses: Vec<String>,
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub enum CallKind {
    Builtin(String),
    User(String),
    Struct(String),
    /// Enum name ("Option" for the built-in) and variant index.
    Variant(String, usize),
}

#[derive(Clone, Debug)]
pub struct CallInfo {
    pub kind: CallKind,
    /// order[i] is the index of the argument supplying parameter i.
    pub order: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct HoleInfo {
    pub id: ExprId,
    pub pos: Pos,
    pub name: String,
    pub expected: Ty,
    pub locals: Vec<(String, Ty)>,
}

pub struct Info {
    pub structs: BTreeMap<String, Vec<(String, RTy)>>,
    pub enums: BTreeMap<String, Vec<VariantInfo>>,
    pub aliases: BTreeMap<String, RTy>,
    pub fns: BTreeMap<String, FnInfo>,
    pub types: HashMap<ExprId, Ty>,
    pub calls: HashMap<ExprId, CallInfo>,
    pub holes: Vec<HoleInfo>,
}

pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [&'static str],
    pub effect: Option<&'static str>,
    pub noreturn: bool,
}

pub const BUILTINS: &[Builtin] = &[
    Builtin { name: "len", params: &["x"], effect: None, noreturn: false },
    Builtin { name: "push", params: &["list", "item"], effect: None, noreturn: false },
    Builtin { name: "slice", params: &["of", "from", "to"], effect: None, noreturn: false },
    Builtin { name: "join", params: &["parts", "sep"], effect: None, noreturn: false },
    Builtin { name: "int_to_str", params: &["n"], effect: None, noreturn: false },
    Builtin { name: "str_to_int", params: &["s"], effect: None, noreturn: false },
    Builtin { name: "char", params: &["code"], effect: None, noreturn: false },
    Builtin { name: "insert", params: &["map", "key", "value"], effect: None, noreturn: false },
    Builtin { name: "lookup", params: &["map", "key"], effect: None, noreturn: false },
    Builtin { name: "has", params: &["map", "key"], effect: None, noreturn: false },
    Builtin { name: "keys", params: &["map"], effect: None, noreturn: false },
    Builtin { name: "hash", params: &["s"], effect: None, noreturn: false },
    Builtin { name: "panic", params: &["msg"], effect: None, noreturn: true },
    Builtin { name: "print", params: &["s"], effect: Some("console"), noreturn: false },
    Builtin { name: "eprint", params: &["s"], effect: Some("console"), noreturn: false },
    Builtin { name: "read_file", params: &["path"], effect: Some("fs"), noreturn: false },
    Builtin { name: "write_file", params: &["path", "contents"], effect: Some("fs"), noreturn: false },
    Builtin { name: "args", params: &[], effect: Some("proc"), noreturn: false },
    Builtin { name: "getenv", params: &["name"], effect: Some("proc"), noreturn: false },
    Builtin { name: "run", params: &["argv", "input"], effect: Some("proc"), noreturn: false },
    Builtin { name: "exit", params: &["code"], effect: Some("proc"), noreturn: true },
];

pub fn builtin(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

const EFFECTS: &[&str] = &["console", "fs", "proc"];
const PRIMITIVE_TYPES: &[&str] = &["Int", "Bool", "String", "Unit", "List", "Map", "Option", "Output"];

impl Info {
    /// The variants of an enum-like type, with field types instantiated.
    pub fn variants(&self, ty: &Ty) -> Option<Vec<VariantInfo>> {
        match ty {
            Ty::Opt(t) => Some(vec![
                VariantInfo { name: "None".into(), fields: vec![] },
                VariantInfo { name: "Some".into(), fields: vec![("value".into(), RTy::plain((**t).clone()))] },
            ]),
            Ty::Enum(n) => self.enums.get(n).cloned(),
            _ => None,
        }
    }

    pub fn ty(&self, e: &Expr) -> Ty {
        self.types.get(&e.id).cloned().unwrap_or(Ty::Unknown)
    }

    /// Resolve a type expression whose validity was already established by the checker.
    pub fn resolve(&self, te: &TypeExpr) -> RTy {
        let mut errs = Vec::new();
        resolve_type(self, te, &mut errs)
    }
}

fn resolve_type(info: &Info, te: &TypeExpr, errs: &mut Vec<Diag>) -> RTy {
    let nargs = |n: usize, errs: &mut Vec<Diag>| {
        if te.args.len() != n {
            errs.push(Diag::new(te.pos, &format!("'{}' takes {} type argument(s), got {}", te.name, n, te.args.len())));
            false
        } else {
            true
        }
    };
    let mut arg = |i: usize, errs: &mut Vec<Diag>| -> Ty {
        let r = resolve_type(info, &te.args[i], errs);
        if !r.preds.is_empty() {
            errs.push(
                Diag::new(te.args[i].pos, "refined types are not allowed inside type arguments")
                    .note("wrap the element in a struct with a refined field instead"),
            );
        }
        r.ty
    };
    let mut base = match te.name.as_str() {
        "Int" | "Bool" | "String" | "Unit" | "Output" => {
            nargs(0, errs);
            let t = match te.name.as_str() {
                "Int" => Ty::Int,
                "Bool" => Ty::Bool,
                "String" => Ty::Str,
                "Unit" => Ty::Unit,
                _ => Ty::Struct("Output".into()),
            };
            RTy::plain(t)
        }
        "List" | "Option" => {
            if nargs(1, errs) {
                let a = arg(0, errs);
                RTy::plain(if te.name == "List" { Ty::List(Box::new(a)) } else { Ty::Opt(Box::new(a)) })
            } else {
                RTy::plain(Ty::Unknown)
            }
        }
        "Map" => {
            if nargs(2, errs) {
                let k = arg(0, errs);
                let v = arg(1, errs);
                if k != Ty::Int && k != Ty::Str && k != Ty::Unknown {
                    errs.push(Diag::new(te.args[0].pos, "map keys must be Int or String"));
                }
                RTy::plain(Ty::Map(Box::new(k), Box::new(v)))
            } else {
                RTy::plain(Ty::Unknown)
            }
        }
        n => {
            if info.structs.contains_key(n) {
                nargs(0, errs);
                RTy::plain(Ty::Struct(n.into()))
            } else if info.enums.contains_key(n) {
                nargs(0, errs);
                RTy::plain(Ty::Enum(n.into()))
            } else if let Some(a) = info.aliases.get(n) {
                nargs(0, errs);
                a.clone()
            } else {
                errs.push(Diag::new(te.pos, &format!("unknown type '{}'", n)));
                RTy::plain(Ty::Unknown)
            }
        }
    };
    if let Some(r) = &te.refine {
        base.preds.push((**r).clone());
    }
    base
}

#[derive(Clone)]
struct Local {
    name: String,
    ty: Ty,
    mutable: bool,
}

pub struct Checker {
    pub info: Info,
    pub errs: Vec<Diag>,
    scopes: Vec<Vec<Local>>,
    uses: Vec<String>,
    ret: Ty,
    loops: usize,
    spec: bool,
    it_ty: Option<Ty>,
    result_ty: Option<Ty>,
}

pub fn check_program(items: &[Item]) -> (Info, Vec<Diag>) {
    let mut structs = BTreeMap::new();
    structs.insert(
        "Output".to_string(),
        vec![("code".to_string(), RTy::plain(Ty::Int)), ("out".to_string(), RTy::plain(Ty::Str))],
    );
    let info = Info {
        structs,
        enums: BTreeMap::new(),
        aliases: BTreeMap::new(),
        fns: BTreeMap::new(),
        types: HashMap::new(),
        calls: HashMap::new(),
        holes: Vec::new(),
    };
    let mut c = Checker {
        info,
        errs: Vec::new(),
        scopes: Vec::new(),
        uses: Vec::new(),
        ret: Ty::Unit,
        loops: 0,
        spec: false,
        it_ty: None,
        result_ty: None,
    };
    c.run(items);
    let Checker { info, errs, .. } = c;
    (info, errs)
}

impl Checker {
    fn err(&mut self, pos: Pos, msg: &str) {
        self.errs.push(Diag::new(pos, msg));
    }

    fn run(&mut self, items: &[Item]) {
        // Phase 1: names.
        let mut seen_types: HashMap<String, Pos> = HashMap::new();
        let mut seen_fns: HashMap<String, Pos> = HashMap::new();
        for it in items {
            let (name, pos, is_type) = match it {
                Item::Fn(f) => (&f.name, f.pos, false),
                Item::Struct(s) => (&s.name, s.pos, true),
                Item::Enum(e) => (&e.name, e.pos, true),
                Item::Alias(a) => (&a.name, a.pos, true),
            };
            if is_type {
                if PRIMITIVE_TYPES.contains(&name.as_str()) {
                    self.err(pos, &format!("'{}' is a built-in type", name));
                } else if let Some(p) = seen_types.get(name) {
                    self.err(pos, &format!("type '{}' is already declared at line {}", name, p.line));
                }
                seen_types.insert(name.clone(), pos);
            } else {
                if builtin(name).is_some() {
                    self.err(pos, &format!("'{}' is a built-in function", name));
                } else if let Some(p) = seen_fns.get(name) {
                    self.err(pos, &format!("function '{}' is already declared at line {}", name, p.line));
                }
                seen_fns.insert(name.clone(), pos);
            }
            match it {
                Item::Struct(s) => {
                    self.info.structs.insert(s.name.clone(), Vec::new());
                }
                Item::Enum(e) => {
                    self.info.enums.insert(e.name.clone(), Vec::new());
                }
                _ => {}
            }
        }
        // Phase 2: aliases (in dependency order), then fields and signatures.
        let alias_decls: Vec<&AliasDecl> =
            items.iter().filter_map(|i| if let Item::Alias(a) = i { Some(a) } else { None }).collect();
        let mut pending: Vec<&AliasDecl> = alias_decls.clone();
        loop {
            let before = pending.len();
            let mut still = Vec::new();
            for a in pending {
                if self.alias_ready(&a.ty, &alias_decls) {
                    let mut errs = Vec::new();
                    let r = resolve_type(&self.info, &a.ty, &mut errs);
                    self.errs.extend(errs);
                    self.info.aliases.insert(a.name.clone(), r);
                } else {
                    still.push(a);
                }
            }
            pending = still;
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        for a in pending {
            self.err(a.pos, &format!("type alias '{}' is defined in terms of itself", a.name));
            self.info.aliases.insert(a.name.clone(), RTy::plain(Ty::Unknown));
        }
        for it in items {
            match it {
                Item::Struct(s) => {
                    let mut fields: Vec<(String, RTy)> = Vec::new();
                    for f in &s.fields {
                        if fields.iter().any(|(n, _)| *n == f.name) {
                            self.err(f.pos, &format!("duplicate field '{}'", f.name));
                        }
                        let r = self.resolve(&f.ty);
                        fields.push((f.name.clone(), r));
                    }
                    self.info.structs.insert(s.name.clone(), fields);
                }
                Item::Enum(e) => {
                    let mut vs: Vec<VariantInfo> = Vec::new();
                    for v in &e.variants {
                        if vs.iter().any(|x| x.name == v.name) {
                            self.err(v.pos, &format!("duplicate variant '{}'", v.name));
                        }
                        let mut fields: Vec<(String, RTy)> = Vec::new();
                        for f in &v.fields {
                            if fields.iter().any(|(n, _)| *n == f.name) {
                                self.err(f.pos, &format!("duplicate field '{}'", f.name));
                            }
                            fields.push((f.name.clone(), self.resolve(&f.ty)));
                        }
                        vs.push(VariantInfo { name: v.name.clone(), fields });
                    }
                    self.info.enums.insert(e.name.clone(), vs);
                }
                _ => {}
            }
        }
        for it in items {
            if let Item::Fn(f) = it {
                let mut params: Vec<(String, RTy)> = Vec::new();
                for p in &f.params {
                    if params.iter().any(|(n, _)| *n == p.name) {
                        self.err(p.pos, &format!("duplicate parameter '{}'", p.name));
                    }
                    params.push((p.name.clone(), self.resolve(&p.ty)));
                }
                let ret = match &f.ret {
                    Some(t) => self.resolve(t),
                    None => RTy::plain(Ty::Unit),
                };
                let mut uses = Vec::new();
                for (u, p) in &f.uses {
                    if !EFFECTS.contains(&u.as_str()) {
                        self.err(*p, &format!("unknown effect '{}' (expected console, fs or proc)", u));
                    }
                    uses.push(u.clone());
                }
                let fi = FnInfo {
                    name: f.name.clone(),
                    params,
                    ret,
                    uses,
                    requires: f.requires.clone(),
                    ensures: f.ensures.clone(),
                    pos: f.pos,
                };
                self.info.fns.insert(f.name.clone(), fi);
            }
        }
        // Phase 3: refinement predicates and contracts.
        for it in items {
            match it {
                Item::Alias(a) => {
                    let base = self.info.aliases.get(&a.name).map(|r| r.ty.clone()).unwrap_or(Ty::Unknown);
                    if let Some(r) = &a.ty.refine {
                        self.check_pred(r, &[], Some(base), None);
                    }
                }
                Item::Struct(s) => {
                    let fields = self.info.structs[&s.name].clone();
                    let scope: Vec<(String, Ty)> = fields.iter().map(|(n, r)| (n.clone(), r.ty.clone())).collect();
                    for (i, f) in s.fields.iter().enumerate() {
                        if let Some(r) = &f.ty.refine {
                            let others: Vec<(String, Ty)> =
                                scope.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, x)| x.clone()).collect();
                            self.check_pred(r, &others, Some(fields[i].1.ty.clone()), None);
                        }
                    }
                }
                Item::Enum(e) => {
                    let vs = self.info.enums[&e.name].clone();
                    for (vi, v) in e.variants.iter().enumerate() {
                        let scope: Vec<(String, Ty)> =
                            vs[vi].fields.iter().map(|(n, r)| (n.clone(), r.ty.clone())).collect();
                        for (i, f) in v.fields.iter().enumerate() {
                            if let Some(r) = &f.ty.refine {
                                let others: Vec<(String, Ty)> = scope
                                    .iter()
                                    .enumerate()
                                    .filter(|(j, _)| *j != i)
                                    .map(|(_, x)| x.clone())
                                    .collect();
                                self.check_pred(r, &others, Some(scope[i].1.clone()), None);
                            }
                        }
                    }
                }
                Item::Fn(f) => {
                    let fi = self.info.fns[&f.name].clone();
                    let all: Vec<(String, Ty)> = fi.params.iter().map(|(n, r)| (n.clone(), r.ty.clone())).collect();
                    for (i, p) in f.params.iter().enumerate() {
                        if let Some(r) = &p.ty.refine {
                            self.check_pred(r, &all[..i], Some(all[i].1.clone()), None);
                        }
                    }
                    if let Some(t) = &f.ret {
                        if let Some(r) = &t.refine {
                            self.check_pred(r, &all, Some(fi.ret.ty.clone()), None);
                        }
                    }
                    for r in &f.requires {
                        self.check_pred(r, &all, None, None);
                    }
                    for e in &f.ensures {
                        self.check_pred(e, &all, None, Some(fi.ret.ty.clone()));
                    }
                }
            }
        }
        // Phase 4: bodies.
        for it in items {
            if let Item::Fn(f) = it {
                self.check_fn(f);
            }
        }
        match self.info.fns.get("main") {
            None => self.err(Pos::default(), "the program has no 'fn main()'"),
            Some(m) => {
                if !m.params.is_empty() || m.ret.ty != Ty::Unit {
                    let p = m.pos;
                    self.err(p, "'main' must take no parameters and return Unit");
                }
            }
        }
    }

    fn alias_ready(&self, te: &TypeExpr, aliases: &[&AliasDecl]) -> bool {
        if aliases.iter().any(|a| a.name == te.name) && !self.info.aliases.contains_key(&te.name) {
            return false;
        }
        te.args.iter().all(|a| self.alias_ready(a, aliases))
    }

    fn resolve(&mut self, te: &TypeExpr) -> RTy {
        let mut errs = Vec::new();
        let r = resolve_type(&self.info, te, &mut errs);
        self.errs.extend(errs);
        r
    }

    /// Check a specification predicate in a scope of named values.
    fn check_pred(&mut self, e: &Expr, scope: &[(String, Ty)], it: Option<Ty>, result: Option<Ty>) {
        let saved = std::mem::take(&mut self.scopes);
        self.scopes =
            vec![scope.iter().map(|(n, t)| Local { name: n.clone(), ty: t.clone(), mutable: false }).collect()];
        self.it_ty = it;
        self.result_ty = result;
        self.spec = true;
        let t = self.expr(e, Some(&Ty::Bool));
        self.expect_ty(e.pos, &t, &Ty::Bool, "a specification");
        self.spec = false;
        self.it_ty = None;
        self.result_ty = None;
        self.scopes = saved;
    }

    fn spec_expr(&mut self, e: &Expr) {
        self.spec = true;
        let t = self.expr(e, Some(&Ty::Bool));
        self.expect_ty(e.pos, &t, &Ty::Bool, "a specification");
        self.spec = false;
    }

    fn expect_ty(&mut self, pos: Pos, got: &Ty, want: &Ty, what: &str) {
        if !compatible(got, want) {
            self.err(pos, &format!("{} must be {}, but this is {}", what, want.show(), got.show()));
        }
    }

    fn lookup(&self, name: &str) -> Option<&Local> {
        for s in self.scopes.iter().rev() {
            for l in s.iter().rev() {
                if l.name == name {
                    return Some(l);
                }
            }
        }
        None
    }

    fn declare(&mut self, pos: Pos, name: &str, ty: Ty, mutable: bool) {
        if self.lookup(name).is_some() {
            self.err(pos, &format!("'{}' is already defined in this scope (Vow has no shadowing)", name));
        }
        self.scopes.last_mut().unwrap().push(Local { name: name.into(), ty, mutable });
    }

    fn check_fn(&mut self, f: &FnDecl) {
        let fi = self.info.fns[&f.name].clone();
        self.scopes = vec![Vec::new()];
        for (n, r) in &fi.params {
            self.scopes[0].push(Local { name: n.clone(), ty: r.ty.clone(), mutable: false });
        }
        self.uses = fi.uses.clone();
        self.ret = fi.ret.ty.clone();
        self.loops = 0;
        self.block(&f.body);
        if fi.ret.ty != Ty::Unit && !terminates(&f.body, &self.info) {
            self.err(f.pos, &format!("function '{}' can finish without returning a {}", f.name, fi.ret.ty.show()));
        }
        self.scopes.clear();
    }

    fn block(&mut self, stmts: &[Stmt]) {
        self.scopes.push(Vec::new());
        for (i, s) in stmts.iter().enumerate() {
            self.stmt(s);
            if i + 1 < stmts.len() && stmt_terminates(s, &self.info) {
                let p = stmts[i + 1].pos;
                self.err(p, "unreachable statement");
            }
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match &s.kind {
            SK::Let { name, ty, value, mutable } => {
                let t = match ty {
                    Some(te) => {
                        let r = self.resolve(te);
                        if let Some(p) = &te.refine {
                            let scope = self.visible();
                            self.check_pred(p, &scope, Some(r.ty.clone()), None);
                        }
                        let got = self.expr(value, Some(&r.ty));
                        self.expect_ty(value.pos, &got, &r.ty, &format!("the value of '{}'", name));
                        r.ty
                    }
                    None => self.expr(value, None),
                };
                if t == Ty::Unit {
                    self.err(s.pos, "cannot bind a Unit value");
                }
                self.declare(s.pos, name, t, *mutable);
            }
            SK::Set { name, value } => match self.lookup(name).cloned() {
                None => self.err(s.pos, &format!("unknown variable '{}'", name)),
                Some(l) => {
                    if !l.mutable {
                        self.err(s.pos, &format!("'{}' is not a var and cannot be reassigned", name));
                    }
                    let got = self.expr(value, Some(&l.ty));
                    self.expect_ty(value.pos, &got, &l.ty, &format!("the new value of '{}'", name));
                }
            },
            SK::Return(v) => {
                let ret = self.ret.clone();
                match v {
                    None => {
                        if ret != Ty::Unit {
                            self.err(s.pos, &format!("this function must return a {}", ret.show()));
                        }
                    }
                    Some(e) => {
                        if ret == Ty::Unit {
                            self.err(s.pos, "this function returns Unit; use a bare 'return'");
                        }
                        let got = self.expr(e, Some(&ret));
                        self.expect_ty(e.pos, &got, &ret, "the returned value");
                    }
                }
            }
            SK::If { arms, els } => {
                for (c, b) in arms {
                    let t = self.expr(c, Some(&Ty::Bool));
                    self.expect_ty(c.pos, &t, &Ty::Bool, "an if condition");
                    self.block(b);
                }
                if let Some(b) = els {
                    self.block(b);
                }
            }
            SK::While { cond, invs, body } => {
                let t = self.expr(cond, Some(&Ty::Bool));
                self.expect_ty(cond.pos, &t, &Ty::Bool, "a while condition");
                for i in invs {
                    self.spec_expr(i);
                }
                self.loops += 1;
                self.block(body);
                self.loops -= 1;
            }
            SK::ForRange { var, lo, hi, invs, body } => {
                let a = self.expr(lo, Some(&Ty::Int));
                self.expect_ty(lo.pos, &a, &Ty::Int, "a range bound");
                let b = self.expr(hi, Some(&Ty::Int));
                self.expect_ty(hi.pos, &b, &Ty::Int, "a range bound");
                for i in invs {
                    self.spec_expr(i);
                }
                self.scopes.push(Vec::new());
                self.declare(s.pos, var, Ty::Int, false);
                self.loops += 1;
                self.block(body);
                self.loops -= 1;
                self.scopes.pop();
            }
            SK::ForEach { var, list, invs, body } => {
                let lt = self.expr(list, None);
                let et = match &lt {
                    Ty::List(t) => (**t).clone(),
                    Ty::Unknown => Ty::Unknown,
                    t => {
                        self.err(list.pos, &format!("'for ... in' needs a List, but this is {}", t.show()));
                        Ty::Unknown
                    }
                };
                for i in invs {
                    self.spec_expr(i);
                }
                self.scopes.push(Vec::new());
                self.declare(s.pos, var, et, false);
                self.loops += 1;
                self.block(body);
                self.loops -= 1;
                self.scopes.pop();
            }
            SK::Match { scrut, cases, els } => self.check_match(s.pos, scrut, cases, els),
            SK::Assert(e) | SK::Check(e) => self.spec_expr(e),
            SK::Break | SK::Continue => {
                if self.loops == 0 {
                    self.err(s.pos, "'break' and 'continue' must be inside a loop");
                }
            }
            SK::Expr(e) => {
                let t = self.expr(e, None);
                if t != Ty::Unit && t != Ty::Unknown {
                    self.err(e.pos, &format!("the {} result of this call is ignored; bind it with 'let'", t.show()));
                }
            }
        }
    }

    fn visible(&self) -> Vec<(String, Ty)> {
        let mut v = Vec::new();
        for s in &self.scopes {
            for l in s {
                v.push((l.name.clone(), l.ty.clone()));
            }
        }
        v
    }

    fn check_match(&mut self, pos: Pos, scrut: &Expr, cases: &[Case], els: &Option<Vec<Stmt>>) {
        let st = self.expr(scrut, None);
        if st == Ty::Unknown {
            return;
        }
        if st == Ty::Int || st == Ty::Str {
            let mut seen: Vec<String> = Vec::new();
            for c in cases {
                let key = match (&c.pat, &st) {
                    (Pat::Int(n), Ty::Int) => n.to_string(),
                    (Pat::Str(s), Ty::Str) => format!("{:?}", s),
                    _ => {
                        self.err(c.pos, &format!("this pattern does not match a {}", st.show()));
                        continue;
                    }
                };
                if seen.contains(&key) {
                    self.err(c.pos, "duplicate case");
                }
                seen.push(key);
                self.block(&c.body);
            }
            match els {
                Some(b) => self.block(b),
                None => self.err(pos, &format!("a match on {} needs an 'else' arm", st.show())),
            }
            return;
        }
        let variants = match self.info.variants(&st) {
            Some(v) => v,
            None => {
                self.err(scrut.pos, &format!("cannot match on a value of type {}", st.show()));
                return;
            }
        };
        let mut covered: Vec<usize> = Vec::new();
        for c in cases {
            match &c.pat {
                Pat::Variant { name, binds } => {
                    let vi = match variants.iter().position(|v| v.name == *name) {
                        Some(i) => i,
                        None => {
                            self.err(c.pos, &format!("{} has no variant '{}'", st.show(), name));
                            continue;
                        }
                    };
                    if covered.contains(&vi) {
                        self.err(c.pos, &format!("duplicate case '{}'", name));
                    }
                    covered.push(vi);
                    self.scopes.push(Vec::new());
                    let mut used: Vec<&String> = Vec::new();
                    for (f, b) in binds {
                        if used.contains(&f) {
                            self.err(c.pos, &format!("field '{}' is bound twice", f));
                        }
                        used.push(f);
                        match variants[vi].fields.iter().find(|(n, _)| n == f) {
                            Some((_, r)) => {
                                let t = r.ty.clone();
                                self.declare(c.pos, b, t, false)
                            }
                            None => self.err(c.pos, &format!("variant '{}' has no field '{}'", name, f)),
                        }
                    }
                    self.block(&c.body);
                    self.scopes.pop();
                }
                _ => self.err(c.pos, &format!("expected a variant of {}", st.show())),
            }
        }
        let all = covered.len() >= variants.len() && (0..variants.len()).all(|i| covered.contains(&i));
        match els {
            Some(b) => {
                if all {
                    self.err(pos, "every variant is already covered; remove the 'else' arm");
                }
                self.block(b);
            }
            None => {
                if !all {
                    let missing: Vec<String> = variants
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !covered.contains(i))
                        .map(|(_, v)| v.name.clone())
                        .collect();
                    self.err(pos, &format!("match is not exhaustive; missing {}", missing.join(", ")));
                }
            }
        }
    }

    fn set_ty(&mut self, e: &Expr, t: Ty) -> Ty {
        self.info.types.insert(e.id, t.clone());
        t
    }

    fn need_effect(&mut self, pos: Pos, eff: &str, what: &str) {
        if !self.uses.iter().any(|u| u == eff) {
            self.errs.push(
                Diag::new(pos, &format!("{} uses '{}', but the enclosing function does not declare it", what, eff))
                    .note(&format!("add 'uses {}' to the function signature", eff)),
            );
        }
    }

    fn match_args(&mut self, pos: Pos, what: &str, params: &[String], args: &[Arg]) -> Option<Vec<usize>> {
        if params.len() == 1 && args.len() == 1 && args[0].name.is_none() {
            return Some(vec![0]);
        }
        let mut order = vec![usize::MAX; params.len()];
        let mut ok = true;
        for (ai, a) in args.iter().enumerate() {
            match &a.name {
                None => {
                    self.err(a.pos, &format!("arguments to {} must be named", what));
                    ok = false;
                }
                Some(n) => match params.iter().position(|p| p == n) {
                    None => {
                        self.err(a.pos, &format!("{} has no parameter '{}'", what, n));
                        ok = false;
                    }
                    Some(pi) => {
                        if order[pi] != usize::MAX {
                            self.err(a.pos, &format!("argument '{}' is given twice", n));
                            ok = false;
                        }
                        order[pi] = ai;
                    }
                },
            }
        }
        for (pi, p) in params.iter().enumerate() {
            if order[pi] == usize::MAX && ok {
                self.err(pos, &format!("missing argument '{}' for {}", p, what));
                ok = false;
            }
        }
        if ok {
            Some(order)
        } else {
            for a in args {
                self.expr(&a.value, None);
            }
            None
        }
    }

    fn spec_forbid(&mut self, e: &Expr, what: &str) -> bool {
        if self.spec {
            self.err(e.pos, &format!("{} is not allowed in a specification", what));
            true
        } else {
            false
        }
    }

    pub fn expr(&mut self, e: &Expr, expected: Option<&Ty>) -> Ty {
        let t = self.expr_inner(e, expected);
        self.set_ty(e, t)
    }

    fn expr_inner(&mut self, e: &Expr, expected: Option<&Ty>) -> Ty {
        match &e.kind {
            EK::Int(_) => Ty::Int,
            EK::Str(_) => Ty::Str,
            EK::Bool(_) => Ty::Bool,
            EK::Var(n) => match self.lookup(n) {
                Some(l) => l.ty.clone(),
                None => {
                    if self.info.enums.contains_key(n) || self.info.structs.contains_key(n) {
                        self.err(e.pos, &format!("'{}' is a type, not a value", n));
                    } else {
                        self.err(e.pos, &format!("unknown variable '{}'", n));
                    }
                    Ty::Unknown
                }
            },
            EK::It => match &self.it_ty {
                Some(t) => t.clone(),
                None => {
                    self.err(e.pos, "'it' is only available inside a 'where' refinement");
                    Ty::Unknown
                }
            },
            EK::Result => match &self.result_ty {
                Some(t) => t.clone(),
                None => {
                    self.err(e.pos, "'result' is only available inside 'ensures'");
                    Ty::Unknown
                }
            },
            EK::Hole(name) => {
                if self.spec_forbid(e, "a hole") {
                    return Ty::Unknown;
                }
                let t = expected.cloned().unwrap_or(Ty::Unknown);
                let locals = self.visible();
                self.info.holes.push(HoleInfo { id: e.id, pos: e.pos, name: name.clone(), expected: t.clone(), locals });
                t
            }
            EK::Call(callee, args) => self.call(e, callee, args, expected),
            EK::Field(obj, name) => {
                if let EK::Var(tn) = &obj.kind {
                    if self.lookup(tn).is_none() && (self.info.enums.contains_key(tn) || tn == "Option") {
                        return self.variant_ctor(e, tn, name, &[], expected, false);
                    }
                }
                let ot = self.expr(obj, None);
                match &ot {
                    Ty::Struct(sn) => match self.info.structs[sn].iter().find(|(n, _)| n == name) {
                        Some((_, r)) => r.ty.clone(),
                        None => {
                            self.err(e.pos, &format!("{} has no field '{}'", sn, name));
                            Ty::Unknown
                        }
                    },
                    Ty::Unknown => Ty::Unknown,
                    t => {
                        self.err(e.pos, &format!("{} has no fields", t.show()));
                        Ty::Unknown
                    }
                }
            }
            EK::Index(obj, idx) => {
                let ot = self.expr(obj, None);
                let it = self.expr(idx, Some(&Ty::Int));
                self.expect_ty(idx.pos, &it, &Ty::Int, "an index");
                match ot {
                    Ty::List(t) => *t,
                    Ty::Str => Ty::Int,
                    Ty::Unknown => Ty::Unknown,
                    t => {
                        self.err(e.pos, &format!("cannot index a {}", t.show()));
                        Ty::Unknown
                    }
                }
            }
            EK::List(items) => {
                if self.spec_forbid(e, "a list literal") {
                    return Ty::Unknown;
                }
                let elem = match expected {
                    Some(Ty::List(t)) => Some((**t).clone()),
                    _ => None,
                };
                if items.is_empty() {
                    return match elem {
                        Some(t) => Ty::List(Box::new(t)),
                        None => {
                            self.err(e.pos, "cannot infer the element type of '[]'; add a type annotation");
                            Ty::Unknown
                        }
                    };
                }
                let first = self.expr(&items[0], elem.as_ref());
                let et = elem.unwrap_or(first.clone());
                self.expect_ty(items[0].pos, &first, &et, "a list element");
                for it in &items[1..] {
                    let t = self.expr(it, Some(&et));
                    self.expect_ty(it.pos, &t, &et, "a list element");
                }
                Ty::List(Box::new(et))
            }
            EK::EmptyMap => {
                if self.spec_forbid(e, "a map literal") {
                    return Ty::Unknown;
                }
                match expected {
                    Some(t @ Ty::Map(..)) => t.clone(),
                    _ => {
                        self.err(e.pos, "cannot infer the type of '{}'; add a type annotation");
                        Ty::Unknown
                    }
                }
            }
            EK::With(obj, args) => {
                if self.spec_forbid(e, "'with'") {
                    return Ty::Unknown;
                }
                let ot = self.expr(obj, expected);
                let sn = match &ot {
                    Ty::Struct(n) => n.clone(),
                    Ty::Unknown => return Ty::Unknown,
                    t => {
                        self.err(e.pos, &format!("'with' needs a struct, but this is {}", t.show()));
                        return Ty::Unknown;
                    }
                };
                let fields = self.info.structs[&sn].clone();
                let mut seen: Vec<String> = Vec::new();
                for a in args {
                    let n = a.name.clone().unwrap_or_default();
                    if seen.contains(&n) {
                        self.err(a.pos, &format!("field '{}' is updated twice", n));
                    }
                    seen.push(n.clone());
                    match fields.iter().find(|(f, _)| *f == n) {
                        Some((_, r)) => {
                            let got = self.expr(&a.value, Some(&r.ty));
                            self.expect_ty(a.value.pos, &got, &r.ty, &format!("field '{}'", n));
                        }
                        None => {
                            self.err(a.pos, &format!("{} has no field '{}'", sn, n));
                            self.expr(&a.value, None);
                        }
                    }
                }
                ot
            }
            EK::Unary(op, x) => {
                let want = if op == "-" { Ty::Int } else { Ty::Bool };
                let t = self.expr(x, Some(&want));
                self.expect_ty(x.pos, &t, &want, &format!("the operand of '{}'", op));
                want
            }
            EK::Binary(op, l, r) => self.binary(e, op, l, r),
        }
    }

    fn binary(&mut self, e: &Expr, op: &str, l: &Expr, r: &Expr) -> Ty {
        match op {
            "+" | "-" | "*" | "/" | "%" => {
                let a = self.expr(l, Some(&Ty::Int));
                let b = self.expr(r, Some(&Ty::Int));
                self.expect_ty(l.pos, &a, &Ty::Int, &format!("the operands of '{}'", op));
                self.expect_ty(r.pos, &b, &Ty::Int, &format!("the operands of '{}'", op));
                Ty::Int
            }
            "<" | "<=" | ">" | ">=" => {
                let a = self.expr(l, Some(&Ty::Int));
                let b = self.expr(r, Some(&Ty::Int));
                self.expect_ty(l.pos, &a, &Ty::Int, &format!("the operands of '{}'", op));
                self.expect_ty(r.pos, &b, &Ty::Int, &format!("the operands of '{}'", op));
                Ty::Bool
            }
            "and" | "or" | "implies" => {
                let a = self.expr(l, Some(&Ty::Bool));
                let b = self.expr(r, Some(&Ty::Bool));
                self.expect_ty(l.pos, &a, &Ty::Bool, &format!("the operands of '{}'", op));
                self.expect_ty(r.pos, &b, &Ty::Bool, &format!("the operands of '{}'", op));
                Ty::Bool
            }
            "==" | "!=" => {
                let a = self.expr(l, None);
                let b = self.expr(r, Some(&a));
                if a != Ty::Unknown && b != Ty::Unknown {
                    if a != b {
                        self.err(e.pos, &format!("cannot compare {} with {}", a.show(), b.show()));
                    } else if a != Ty::Int && a != Ty::Bool && a != Ty::Str {
                        self.err(
                            e.pos,
                            &format!("'{}' works on Int, Bool and String, not {}; use match instead", op, a.show()),
                        );
                    }
                }
                Ty::Bool
            }
            "++" => {
                if self.spec_forbid(e, "'++'") {
                    return Ty::Unknown;
                }
                let a = self.expr(l, None);
                let b = self.expr(r, Some(&a));
                match &a {
                    Ty::Str | Ty::List(_) => {
                        self.expect_ty(r.pos, &b, &a, "the right side of '++'");
                        a
                    }
                    Ty::Unknown => b,
                    t => {
                        self.err(l.pos, &format!("'++' joins Strings or Lists, not {}", t.show()));
                        Ty::Unknown
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    fn variant_ctor(
        &mut self,
        e: &Expr,
        tn: &str,
        vn: &str,
        args: &[Arg],
        expected: Option<&Ty>,
        called: bool,
    ) -> Ty {
        let ty = if tn == "Option" {
            match expected {
                Some(t @ Ty::Opt(_)) => t.clone(),
                _ => {
                    if vn == "Some" && args.len() == 1 {
                        let inner = self.expr(&args[0].value, None);
                        Ty::Opt(Box::new(inner))
                    } else {
                        self.err(e.pos, "cannot infer the type of this Option; add a type annotation");
                        return Ty::Unknown;
                    }
                }
            }
        } else {
            Ty::Enum(tn.to_string())
        };
        let variants = self.info.variants(&ty).unwrap();
        let vi = match variants.iter().position(|v| v.name == vn) {
            Some(i) => i,
            None => {
                self.err(e.pos, &format!("{} has no variant '{}'", tn, vn));
                return Ty::Unknown;
            }
        };
        let fields = variants[vi].fields.clone();
        if !called && !fields.is_empty() {
            self.err(e.pos, &format!("variant '{}' has fields; construct it with {}.{}(...)", vn, tn, vn));
            return Ty::Unknown;
        }
        if called && fields.is_empty() {
            self.err(e.pos, &format!("variant '{}' has no fields; write {}.{} without parentheses", vn, tn, vn));
            return Ty::Unknown;
        }
        let names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
        let what = format!("variant '{}'", vn);
        if let Some(order) = self.match_args(e.pos, &what, &names, args) {
            for (pi, (fname, r)) in fields.iter().enumerate() {
                let a = &args[order[pi]];
                if self.info.types.contains_key(&a.value.id) && tn == "Option" && expected.is_none() {
                    continue; // already typed while inferring Option.Some
                }
                let got = self.expr(&a.value, Some(&r.ty));
                self.expect_ty(a.value.pos, &got, &r.ty, &format!("field '{}'", fname));
            }
            self.info.calls.insert(e.id, CallInfo { kind: CallKind::Variant(tn.to_string(), vi), order });
        }
        ty
    }

    fn call(&mut self, e: &Expr, callee: &Expr, args: &[Arg], expected: Option<&Ty>) -> Ty {
        let name = match &callee.kind {
            EK::Var(n) => n.clone(),
            EK::Field(obj, vn) => {
                if let EK::Var(tn) = &obj.kind {
                    if self.lookup(tn).is_none() && (self.info.enums.contains_key(tn) || tn == "Option") {
                        if self.spec_forbid(e, "constructing a value") {
                            return Ty::Unknown;
                        }
                        return self.variant_ctor(e, tn, vn, args, expected, true);
                    }
                }
                self.err(e.pos, "only named functions can be called");
                return Ty::Unknown;
            }
            _ => {
                self.err(e.pos, "only named functions can be called");
                return Ty::Unknown;
            }
        };
        if let Some(b) = builtin(&name) {
            if self.spec && name != "len" {
                self.err(e.pos, &format!("'{}' is not allowed in a specification (only 'len' is)", name));
                return Ty::Unknown;
            }
            if let Some(eff) = b.effect {
                self.need_effect(e.pos, eff, &format!("'{}'", name));
            }
            let params: Vec<String> = b.params.iter().map(|s| s.to_string()).collect();
            let order = match self.match_args(e.pos, &format!("'{}'", name), &params, args) {
                Some(o) => o,
                None => return Ty::Unknown,
            };
            self.info.calls.insert(e.id, CallInfo { kind: CallKind::Builtin(name.clone()), order: order.clone() });
            let a: Vec<&Expr> = order.iter().map(|i| &args[*i].value).collect();
            return self.builtin_call(e, &name, &a, expected);
        }
        if let Some(fi) = self.info.fns.get(&name).cloned() {
            if self.spec_forbid(e, &format!("calling '{}'", name)) {
                return Ty::Unknown;
            }
            for u in &fi.uses {
                self.need_effect(e.pos, u, &format!("'{}'", name));
            }
            let names: Vec<String> = fi.params.iter().map(|(n, _)| n.clone()).collect();
            if let Some(order) = self.match_args(e.pos, &format!("'{}'", name), &names, args) {
                self.typed_args(args, &order, &fi.params);
                self.info.calls.insert(e.id, CallInfo { kind: CallKind::User(name.clone()), order });
            }
            return fi.ret.ty.clone();
        }
        if let Some(fields) = self.info.structs.get(&name).cloned() {
            if self.spec_forbid(e, "constructing a value") {
                return Ty::Unknown;
            }
            let names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            if let Some(order) = self.match_args(e.pos, &format!("struct '{}'", name), &names, args) {
                self.typed_args(args, &order, &fields);
                self.info.calls.insert(e.id, CallInfo { kind: CallKind::Struct(name.clone()), order });
            }
            return Ty::Struct(name);
        }
        if self.info.enums.contains_key(&name) {
            self.err(e.pos, &format!("'{}' is an enum; construct a variant with {}.Variant(...)", name, name));
        } else {
            self.err(e.pos, &format!("unknown function '{}'", name));
        }
        for a in args {
            self.expr(&a.value, None);
        }
        Ty::Unknown
    }

    fn typed_args(&mut self, args: &[Arg], order: &[usize], params: &[(String, RTy)]) {
        // Evaluate in written order, checking against the parameter each argument fills.
        for (ai, a) in args.iter().enumerate() {
            let pi = order.iter().position(|x| *x == ai).unwrap();
            let (pn, r) = &params[pi];
            let got = self.expr(&a.value, Some(&r.ty));
            self.expect_ty(a.value.pos, &got, &r.ty, &format!("argument '{}'", pn));
        }
    }

    fn builtin_call(&mut self, e: &Expr, name: &str, a: &[&Expr], expected: Option<&Ty>) -> Ty {
        let int = Ty::Int;
        let st = Ty::Str;
        let mut want = |c: &mut Checker, x: &Expr, t: &Ty, what: &str| {
            let got = c.expr(x, Some(t));
            c.expect_ty(x.pos, &got, t, what);
        };
        match name {
            "len" => {
                let t = self.expr(a[0], None);
                match t {
                    Ty::List(_) | Ty::Str | Ty::Map(..) | Ty::Unknown => {}
                    t => self.err(a[0].pos, &format!("'len' needs a List, String or Map, not {}", t.show())),
                }
                Ty::Int
            }
            "push" => {
                let lt = self.expr(a[0], expected);
                match &lt {
                    Ty::List(t) => {
                        let t = (**t).clone();
                        want(self, a[1], &t, "the pushed item");
                        lt
                    }
                    Ty::Unknown => {
                        self.expr(a[1], None);
                        Ty::Unknown
                    }
                    t => {
                        self.err(a[0].pos, &format!("'push' needs a List, not {}", t.show()));
                        Ty::Unknown
                    }
                }
            }
            "slice" => {
                let t = self.expr(a[0], expected);
                want(self, a[1], &int, "'from'");
                want(self, a[2], &int, "'to'");
                match &t {
                    Ty::List(_) | Ty::Str | Ty::Unknown => t,
                    t => {
                        self.err(a[0].pos, &format!("'slice' needs a List or String, not {}", t.show()));
                        Ty::Unknown
                    }
                }
            }
            "join" => {
                want(self, a[0], &Ty::List(Box::new(Ty::Str)), "'parts'");
                want(self, a[1], &st, "'sep'");
                Ty::Str
            }
            "int_to_str" => {
                want(self, a[0], &int, "the argument");
                Ty::Str
            }
            "str_to_int" => {
                want(self, a[0], &st, "the argument");
                Ty::Opt(Box::new(Ty::Int))
            }
            "char" => {
                want(self, a[0], &int, "the character code");
                Ty::Str
            }
            "hash" => {
                want(self, a[0], &st, "the argument");
                Ty::Str
            }
            "insert" | "lookup" | "has" | "keys" => {
                let mt = self.expr(a[0], if name == "insert" { expected } else { None });
                let (k, v) = match &mt {
                    Ty::Map(k, v) => ((**k).clone(), (**v).clone()),
                    Ty::Unknown => {
                        for x in &a[1..] {
                            self.expr(x, None);
                        }
                        return Ty::Unknown;
                    }
                    t => {
                        self.err(a[0].pos, &format!("'{}' needs a Map, not {}", name, t.show()));
                        return Ty::Unknown;
                    }
                };
                if name != "keys" {
                    want(self, a[1], &k, "the key");
                }
                match name {
                    "insert" => {
                        want(self, a[2], &v, "the value");
                        mt.clone()
                    }
                    "lookup" => Ty::Opt(Box::new(v)),
                    "has" => Ty::Bool,
                    _ => Ty::List(Box::new(k)),
                }
            }
            "panic" | "print" | "eprint" => {
                want(self, a[0], &st, "the message");
                Ty::Unit
            }
            "read_file" => {
                want(self, a[0], &st, "the path");
                Ty::Opt(Box::new(Ty::Str))
            }
            "write_file" => {
                want(self, a[0], &st, "the path");
                want(self, a[1], &st, "the contents");
                Ty::Bool
            }
            "args" => Ty::List(Box::new(Ty::Str)),
            "getenv" => {
                want(self, a[0], &st, "the variable name");
                Ty::Opt(Box::new(Ty::Str))
            }
            "run" => {
                want(self, a[0], &Ty::List(Box::new(Ty::Str)), "'argv'");
                want(self, a[1], &st, "'input'");
                Ty::Struct("Output".into())
            }
            "exit" => {
                want(self, a[0], &int, "the exit code");
                Ty::Unit
            }
            _ => {
                self.err(e.pos, &format!("unhandled builtin '{}'", name));
                Ty::Unknown
            }
        }
    }
}

pub fn compatible(got: &Ty, want: &Ty) -> bool {
    got == want || *got == Ty::Unknown || *want == Ty::Unknown
}

fn is_noreturn_call(e: &Expr) -> bool {
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
        _ => false, // breaks inside nested loops belong to those loops
    })
}

pub fn stmt_terminates(s: &Stmt, info: &Info) -> bool {
    match &s.kind {
        SK::Return(_) | SK::Break | SK::Continue => true,
        SK::Expr(e) => is_noreturn_call(e),
        SK::If { arms, els } => match els {
            Some(b) => arms.iter().all(|(_, a)| terminates(a, info)) && terminates(b, info),
            None => false,
        },
        SK::Match { cases, els, .. } => {
            cases.iter().all(|c| terminates(&c.body, info)) && els.as_ref().map_or(true, |b| terminates(b, info))
        }
        SK::While { cond, body, .. } => matches!(cond.kind, EK::Bool(true)) && !has_break(body),
        _ => false,
    }
}

/// True if control cannot fall off the end of the block. `break`/`continue` count as
/// leaving the block, which is what the checker needs for "missing return" because the
/// enclosing loop is never the last statement of a function returning a value unless
/// it is `while true` without a break.
pub fn terminates(stmts: &[Stmt], info: &Info) -> bool {
    stmts.iter().any(|s| stmt_terminates(s, info))
}
