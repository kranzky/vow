// Content addressing. A function's body hash covers its canonical text. Its
// verification key also covers the contracts of everything it calls and all type
// declarations, which is exactly what its proof depends on: callers rely on a callee's
// contract, never on its body.

use crate::ast::*;
use crate::check::{CallKind, Info};
use crate::pretty;
use std::collections::BTreeSet;

/// Bump when the verifier's encoding changes, so stale cache entries are ignored.
pub const VERIFIER_VERSION: &str = "vow-verifier-1";

pub fn fnv(s: &str) -> String {
    let mut h: u64 = 14695981039346656037;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    format!("{:016x}", h)
}

fn calls_in_expr(e: &Expr, info: &Info, out: &mut BTreeSet<String>) {
    if let Some(ci) = info.calls.get(&e.id) {
        if let CallKind::User(n) = &ci.kind {
            out.insert(n.clone());
        }
    }
    match &e.kind {
        EK::Call(c, args) | EK::With(c, args) => {
            calls_in_expr(c, info, out);
            for a in args {
                calls_in_expr(&a.value, info, out);
            }
        }
        EK::List(items) => {
            for i in items {
                calls_in_expr(i, info, out);
            }
        }
        EK::Index(a, b) | EK::Binary(_, a, b) => {
            calls_in_expr(a, info, out);
            calls_in_expr(b, info, out);
        }
        EK::Field(a, _) | EK::Unary(_, a) => calls_in_expr(a, info, out),
        _ => {}
    }
}

fn calls_in_block(stmts: &[Stmt], info: &Info, out: &mut BTreeSet<String>) {
    for s in stmts {
        match &s.kind {
            SK::Let { value, .. } | SK::Set { value, .. } => calls_in_expr(value, info, out),
            SK::Return(Some(e)) | SK::Assert(e) | SK::Check(e) | SK::Expr(e) => calls_in_expr(e, info, out),
            SK::If { arms, els } => {
                for (c, b) in arms {
                    calls_in_expr(c, info, out);
                    calls_in_block(b, info, out);
                }
                if let Some(b) = els {
                    calls_in_block(b, info, out);
                }
            }
            SK::While { cond, body, .. } => {
                calls_in_expr(cond, info, out);
                calls_in_block(body, info, out);
            }
            SK::ForRange { lo, hi, body, .. } => {
                calls_in_expr(lo, info, out);
                calls_in_expr(hi, info, out);
                calls_in_block(body, info, out);
            }
            SK::ForEach { list, body, .. } => {
                calls_in_expr(list, info, out);
                calls_in_block(body, info, out);
            }
            SK::Match { scrut, cases, els } => {
                calls_in_expr(scrut, info, out);
                for c in cases {
                    calls_in_block(&c.body, info, out);
                }
                if let Some(b) = els {
                    calls_in_block(b, info, out);
                }
            }
            _ => {}
        }
    }
}

pub struct FnHash {
    pub name: String,
    pub body: String,
    pub contract: String,
    pub key: String,
}

pub fn hash_program(items: &[Item], info: &Info) -> Vec<FnHash> {
    let mut types = String::new();
    for it in items {
        if !matches!(it, Item::Fn(_)) {
            types.push_str(&pretty::item(it));
            types.push('\n');
        }
    }
    let types_hash = fnv(&types);
    let mut contracts = std::collections::HashMap::new();
    for it in items {
        if let Item::Fn(f) = it {
            contracts.insert(f.name.clone(), fnv(&pretty::fn_contract(f)));
        }
    }
    let mut out = Vec::new();
    for it in items {
        if let Item::Fn(f) = it {
            let body = fnv(&pretty::item(it));
            let mut callees = BTreeSet::new();
            calls_in_block(&f.body, info, &mut callees);
            let mut material = format!("{}\n{}\n{}\n", VERIFIER_VERSION, body, types_hash);
            for c in &callees {
                material.push_str(&format!("{}={}\n", c, contracts[c]));
            }
            out.push(FnHash { name: f.name.clone(), body, contract: contracts[&f.name].clone(), key: fnv(&material) });
        }
    }
    out
}
