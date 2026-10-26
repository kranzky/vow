use crate::ast::*;

fn quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            '\0' => o.push_str("\\0"),
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn args(a: &[Arg]) -> String {
    let parts: Vec<String> = a
        .iter()
        .map(|x| match &x.name {
            Some(n) => format!("{}: {}", n, expr(&x.value)),
            None => expr(&x.value),
        })
        .collect();
    parts.join(", ")
}

fn operand(e: &Expr) -> String {
    match &e.kind {
        EK::Binary(..) => format!("({})", expr(e)),
        _ => expr(e),
    }
}

pub fn expr(e: &Expr) -> String {
    match &e.kind {
        EK::Int(n) => n.to_string(),
        EK::Str(s) => quote(s),
        EK::Bool(b) => b.to_string(),
        EK::Var(n) => n.clone(),
        EK::It => "it".into(),
        EK::Result => "result".into(),
        EK::Hole(n) => format!("?{}", n),
        EK::Call(c, a) => format!("{}({})", expr(c), args(a)),
        EK::List(items) => {
            let parts: Vec<String> = items.iter().map(expr).collect();
            format!("[{}]", parts.join(", "))
        }
        EK::EmptyMap => "{}".into(),
        EK::Index(o, i) => format!("{}[{}]", operand(o), expr(i)),
        EK::Field(o, f) => format!("{}.{}", operand(o), f),
        EK::With(o, a) => format!("{} with ({})", operand(o), args(a)),
        EK::Unary(op, x) => {
            if op == "not" {
                format!("not {}", operand(x))
            } else {
                format!("-{}", operand(x))
            }
        }
        EK::Binary(op, l, r) => format!("{} {} {}", operand(l), op, operand(r)),
    }
}

pub fn ty(t: &TypeExpr) -> String {
    let mut s = t.name.clone();
    if !t.args.is_empty() {
        let parts: Vec<String> = t.args.iter().map(ty).collect();
        s.push_str(&format!("<{}>", parts.join(", ")));
    }
    if let Some(r) = &t.refine {
        s.push_str(&format!(" where {}", expr(r)));
    }
    s
}

fn fields(fs: &[Field]) -> String {
    let parts: Vec<String> = fs.iter().map(|f| format!("{}: {}", f.name, ty(&f.ty))).collect();
    parts.join(", ")
}

fn block(out: &mut Vec<String>, stmts: &[Stmt], ind: usize) {
    for s in stmts {
        stmt(out, s, ind);
    }
}

fn line(out: &mut Vec<String>, ind: usize, s: String) {
    out.push(format!("{}{}", "  ".repeat(ind), s));
}

fn invs(v: &[Expr]) -> String {
    v.iter().map(|i| format!(" invariant {}", expr(i))).collect::<Vec<_>>().join("")
}

fn stmt(out: &mut Vec<String>, s: &Stmt, ind: usize) {
    match &s.kind {
        SK::Let { name, ty: t, value, mutable } => {
            let kw = if *mutable { "var" } else { "let" };
            match t {
                Some(t) => line(out, ind, format!("{} {}: {} = {}", kw, name, ty(t), expr(value))),
                None => line(out, ind, format!("{} {} = {}", kw, name, expr(value))),
            }
        }
        SK::Set { name, value } => line(out, ind, format!("set {} = {}", name, expr(value))),
        SK::Return(None) => line(out, ind, "return".into()),
        SK::Return(Some(e)) => line(out, ind, format!("return {}", expr(e))),
        SK::If { arms, els } => {
            for (i, (c, b)) in arms.iter().enumerate() {
                let kw = if i == 0 { "if" } else { "elif" };
                line(out, ind, format!("{} {} then", kw, expr(c)));
                block(out, b, ind + 1);
            }
            if let Some(b) = els {
                line(out, ind, "else".into());
                block(out, b, ind + 1);
            }
            line(out, ind, "end if".into());
        }
        SK::While { cond, invs: iv, body } => {
            line(out, ind, format!("while {}{} do", expr(cond), invs(iv)));
            block(out, body, ind + 1);
            line(out, ind, "end while".into());
        }
        SK::ForRange { var, lo, hi, invs: iv, body } => {
            line(out, ind, format!("for {} in {}..{}{} do", var, expr(lo), expr(hi), invs(iv)));
            block(out, body, ind + 1);
            line(out, ind, "end for".into());
        }
        SK::ForEach { var, list, invs: iv, body } => {
            line(out, ind, format!("for {} in {}{} do", var, expr(list), invs(iv)));
            block(out, body, ind + 1);
            line(out, ind, "end for".into());
        }
        SK::Match { scrut, cases, els } => {
            line(out, ind, format!("match {}", expr(scrut)));
            for c in cases {
                let p = match &c.pat {
                    Pat::Int(n) => n.to_string(),
                    Pat::Str(s) => quote(s),
                    Pat::Variant { name, binds } => {
                        if binds.is_empty() {
                            name.clone()
                        } else {
                            let bs: Vec<String> = binds.iter().map(|(f, b)| format!("{}: {}", f, b)).collect();
                            format!("{}({})", name, bs.join(", "))
                        }
                    }
                };
                line(out, ind, format!("case {} then", p));
                block(out, &c.body, ind + 1);
            }
            if let Some(b) = els {
                line(out, ind, "else".into());
                block(out, b, ind + 1);
            }
            line(out, ind, "end match".into());
        }
        SK::Assert(e) => line(out, ind, format!("assert {}", expr(e))),
        SK::Check(e) => line(out, ind, format!("check {}", expr(e))),
        SK::Break => line(out, ind, "break".into()),
        SK::Continue => line(out, ind, "continue".into()),
        SK::Expr(e) => line(out, ind, expr(e)),
    }
}

/// The signature and contract of a function: everything a caller relies on.
pub fn fn_contract(f: &FnDecl) -> String {
    let mut s = format!("fn {}({})", f.name, fields(&f.params));
    if let Some(r) = &f.ret {
        s.push_str(&format!(" -> {}", ty(r)));
    }
    if !f.uses.is_empty() {
        let u: Vec<String> = f.uses.iter().map(|(n, _)| n.clone()).collect();
        s.push_str(&format!(" uses {}", u.join(", ")));
    }
    for r in &f.requires {
        s.push_str(&format!(" requires {}", expr(r)));
    }
    for e in &f.ensures {
        s.push_str(&format!(" ensures {}", expr(e)));
    }
    s
}

pub fn item(i: &Item) -> String {
    let mut out = Vec::new();
    match i {
        Item::Fn(f) => {
            out.push(format!("{} do", fn_contract(f)));
            block(&mut out, &f.body, 1);
            out.push(format!("end fn {}", f.name));
        }
        Item::Struct(s) => {
            out.push(format!("struct {}", s.name));
            for f in &s.fields {
                out.push(format!("  {}: {},", f.name, ty(&f.ty)));
            }
            out.push(format!("end struct {}", s.name));
        }
        Item::Enum(e) => {
            out.push(format!("enum {}", e.name));
            for v in &e.variants {
                if v.fields.is_empty() {
                    out.push(format!("  {},", v.name));
                } else {
                    out.push(format!("  {}({}),", v.name, fields(&v.fields)));
                }
            }
            out.push(format!("end enum {}", e.name));
        }
        Item::Alias(a) => out.push(format!("type {} = {}", a.name, ty(&a.ty))),
    }
    out.join("\n")
}
