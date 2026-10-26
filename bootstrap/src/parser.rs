use crate::ast::*;
use crate::diag::{Diag, Pos};
use crate::lexer::{Token, TK};

pub struct Parser<'a> {
    toks: &'a [Token],
    pos: usize,
    pub next_id: usize,
}

pub struct Parsed {
    pub items: Vec<Item>,
    pub imports: Vec<(String, Pos)>,
}

const BINOPS: &[&str] =
    &["+", "-", "*", "/", "%", "++", "==", "!=", "<", "<=", ">", ">=", "and", "or", "implies"];
const CHAINABLE: &[&str] = &["+", "*", "++", "and", "or"];
const BLOCK_END: &[&str] = &["end", "else", "elif", "case"];

type PResult<T> = Result<T, Diag>;

fn describe(t: &Token) -> String {
    match t.kind {
        TK::Eof => "end of file".to_string(),
        TK::Str => format!("string \"{}\"", t.text),
        TK::Hole => format!("hole ?{}", t.text),
        _ => format!("'{}'", t.text),
    }
}

impl<'a> Parser<'a> {
    pub fn new(toks: &'a [Token], first_id: usize) -> Parser<'a> {
        Parser { toks, pos: 0, next_id: first_id }
    }

    fn peek(&self) -> &Token {
        &self.toks[self.pos]
    }

    fn peek_at(&self, n: usize) -> &Token {
        let i = (self.pos + n).min(self.toks.len() - 1);
        &self.toks[i]
    }

    fn advance(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn is(&self, text: &str) -> bool {
        let t = self.peek();
        (t.kind == TK::Sym || t.kind == TK::Keyword) && t.text == text
    }

    fn accept(&mut self, text: &str) -> bool {
        if self.is(text) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, text: &str, ctx: &str) -> PResult<Token> {
        if self.is(text) {
            Ok(self.advance())
        } else {
            let t = self.peek();
            Err(Diag::new(t.pos, &format!("expected '{}' {}, found {}", text, ctx, describe(t))))
        }
    }

    fn ident(&mut self, ctx: &str) -> PResult<(String, Pos)> {
        let t = self.peek().clone();
        if t.kind == TK::Ident {
            self.advance();
            Ok((t.text, t.pos))
        } else if t.kind == TK::Keyword {
            Err(Diag::new(t.pos, &format!("expected a name {}, found {}", ctx, describe(&t))).note(&format!(
                "'{}' is a keyword: keywords can label fields and arguments, but cannot name variables, parameters, functions or types",
                t.text
            )))
        } else {
            Err(Diag::new(t.pos, &format!("expected a name {}, found {}", ctx, describe(&t))))
        }
    }

    /// A label: any identifier or keyword (field names, argument labels).
    fn label(&mut self, ctx: &str) -> PResult<(String, Pos)> {
        let t = self.peek().clone();
        if t.kind == TK::Ident || t.kind == TK::Keyword {
            self.advance();
            Ok((t.text, t.pos))
        } else {
            Err(Diag::new(t.pos, &format!("expected a name {}, found {}", ctx, describe(&t))))
        }
    }

    fn mk(&mut self, pos: Pos, kind: EK) -> Expr {
        let id = self.next_id;
        self.next_id += 1;
        Expr { id, pos, kind }
    }

    fn close(&mut self, what: &str, name: &str, opened: Pos) -> PResult<()> {
        let t = self.peek().clone();
        let label = if name.is_empty() { what.to_string() } else { format!("{} {}", what, name) };
        let mismatch = || {
            Diag::new(t.pos, &format!("expected 'end {}' to close the {} opened at line {}", label, what, opened.line))
        };
        if !self.is("end") {
            return Err(mismatch().note(&format!("found {}", describe(&t))));
        }
        self.advance();
        let k = self.peek().clone();
        if !((k.kind == TK::Keyword) && k.text == what) {
            return Err(mismatch().note(&format!("found 'end {}'", k.text)));
        }
        self.advance();
        if !name.is_empty() {
            let n = self.peek().clone();
            if n.kind != TK::Ident || n.text != name {
                return Err(mismatch().note(&format!("found 'end {} {}'", what, n.text)));
            }
            self.advance();
        }
        Ok(())
    }

    pub fn parse_file(&mut self) -> PResult<Parsed> {
        let mut items = Vec::new();
        let mut imports = Vec::new();
        while self.peek().kind != TK::Eof {
            let t = self.peek().clone();
            if self.accept("import") {
                let p = self.advance();
                if p.kind != TK::Str {
                    return Err(Diag::new(p.pos, "expected a string path after 'import'"));
                }
                imports.push((p.text, t.pos));
            } else if self.is("fn") {
                items.push(Item::Fn(self.parse_fn()?));
            } else if self.is("struct") {
                items.push(Item::Struct(self.parse_struct()?));
            } else if self.is("enum") {
                items.push(Item::Enum(self.parse_enum()?));
            } else if self.accept("type") {
                let (name, _) = self.ident("for the type alias")?;
                self.expect("=", "after the alias name")?;
                let ty = self.parse_type()?;
                items.push(Item::Alias(AliasDecl { name, ty, pos: t.pos }));
            } else {
                return Err(Diag::new(
                    t.pos,
                    &format!("expected a declaration (fn, struct, enum, type, import), found {}", describe(&t)),
                ));
            }
        }
        Ok(Parsed { items, imports })
    }

    fn parse_type(&mut self) -> PResult<TypeExpr> {
        let (name, pos) = self.ident("for a type")?;
        let mut args = Vec::new();
        if self.accept("<") {
            loop {
                let a = self.parse_type_noref()?;
                args.push(a);
                if !self.accept(",") {
                    break;
                }
            }
            self.expect(">", "to close the type arguments")?;
        }
        let refine = if self.accept("where") { Some(Box::new(self.parse_expr()?)) } else { None };
        Ok(TypeExpr { name, args, refine, pos })
    }

    fn parse_type_noref(&mut self) -> PResult<TypeExpr> {
        let t = self.parse_type()?;
        if let Some(r) = &t.refine {
            return Err(Diag::new(r.pos, "refinements are not allowed inside type arguments")
                .note("wrap the element in a struct with a refined field instead"));
        }
        Ok(t)
    }

    fn parse_fields(&mut self, closer: &str, labels: bool) -> PResult<Vec<Field>> {
        let mut fields = Vec::new();
        while !self.is(closer) {
            let (name, pos) =
                if labels { self.label("for a field")? } else { self.ident("for a field or parameter")? };
            self.expect(":", "after the field name")?;
            let inout = self.accept("inout");
            let ty = self.parse_type()?;
            fields.push(Field { name, inout, ty, pos });
            if !self.accept(",") {
                break;
            }
        }
        Ok(fields)
    }

    fn parse_fn(&mut self) -> PResult<FnDecl> {
        let start = self.expect("fn", "")?.pos;
        let (name, _) = self.ident("for the function")?;
        self.expect("(", "to open the parameter list")?;
        let params = self.parse_fields(")", false)?;
        self.expect(")", "to close the parameter list")?;
        let ret = if self.accept("->") { Some(self.parse_type()?) } else { None };
        let mut uses = Vec::new();
        if self.accept("uses") {
            loop {
                let (e, p) = self.ident("for an effect")?;
                uses.push((e, p));
                if !self.accept(",") {
                    break;
                }
            }
        }
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        let mut decreases = None;
        loop {
            if self.accept("requires") {
                requires.push(self.parse_expr()?);
            } else if self.accept("ensures") {
                ensures.push(self.parse_expr()?);
            } else if self.is("decreases") {
                let p = self.advance().pos;
                if decreases.is_some() {
                    return Err(Diag::new(p, "a function has at most one 'decreases' clause"));
                }
                decreases = Some(self.parse_expr()?);
            } else {
                break;
            }
        }
        self.expect("do", "to begin the function body")?;
        let body = self.parse_block()?;
        self.close("fn", &name, start)?;
        Ok(FnDecl { name, params, ret, uses, requires, ensures, decreases, body, pos: start })
    }

    fn parse_struct(&mut self) -> PResult<StructDecl> {
        let start = self.expect("struct", "")?.pos;
        let (name, _) = self.ident("for the struct")?;
        let fields = self.parse_fields("end", true)?;
        self.close("struct", &name, start)?;
        Ok(StructDecl { name, fields, pos: start })
    }

    fn parse_enum(&mut self) -> PResult<EnumDecl> {
        let start = self.expect("enum", "")?.pos;
        let (name, _) = self.ident("for the enum")?;
        let mut variants = Vec::new();
        while !self.is("end") {
            let (vname, vpos) = self.ident("for a variant")?;
            let mut fields = Vec::new();
            if self.accept("(") {
                fields = self.parse_fields(")", true)?;
                self.expect(")", "to close the variant fields")?;
            }
            variants.push(Variant { name: vname, fields, pos: vpos });
            if !self.accept(",") {
                break;
            }
        }
        self.close("enum", &name, start)?;
        Ok(EnumDecl { name, variants, pos: start })
    }

    fn at_block_end(&self) -> bool {
        let t = self.peek();
        t.kind == TK::Eof || (t.kind == TK::Keyword && BLOCK_END.contains(&t.text.as_str()))
    }

    fn parse_block(&mut self) -> PResult<Vec<Stmt>> {
        let mut out = Vec::new();
        while !self.at_block_end() {
            out.push(self.parse_stmt()?);
        }
        Ok(out)
    }

    fn parse_invariants(&mut self) -> PResult<Vec<Expr>> {
        let mut invs = Vec::new();
        while self.accept("invariant") {
            invs.push(self.parse_expr()?);
        }
        Ok(invs)
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        let t = self.peek().clone();
        let pos = t.pos;
        let kind = if t.kind == TK::Keyword {
            match t.text.as_str() {
                "let" | "var" => {
                    self.advance();
                    let (name, _) = self.ident("for the binding")?;
                    let ty = if self.accept(":") { Some(self.parse_type()?) } else { None };
                    self.expect("=", "in the binding")?;
                    let value = self.parse_expr()?;
                    SK::Let { name, ty, value, mutable: t.text == "var" }
                }
                "set" => {
                    self.advance();
                    let (name, _) = self.ident("after 'set'")?;
                    self.expect("=", "in the assignment")?;
                    SK::Set { name, value: self.parse_expr()? }
                }
                "return" => {
                    self.advance();
                    let n = self.peek();
                    let bare = n.kind == TK::Eof
                        || (n.kind == TK::Keyword
                            && [
                                "end", "else", "elif", "case", "let", "var", "set", "return", "if", "while",
                                "for", "match", "assert", "check", "break", "continue",
                            ]
                            .contains(&n.text.as_str()));
                    SK::Return(if bare { None } else { Some(self.parse_expr()?) })
                }
                "if" => {
                    self.advance();
                    let mut arms = Vec::new();
                    let c = self.parse_expr()?;
                    self.expect("then", "after the if condition")?;
                    arms.push((c, self.parse_block()?));
                    let mut els = None;
                    loop {
                        if self.accept("elif") {
                            let c = self.parse_expr()?;
                            self.expect("then", "after the elif condition")?;
                            arms.push((c, self.parse_block()?));
                        } else if self.accept("else") {
                            els = Some(self.parse_block()?);
                            break;
                        } else {
                            break;
                        }
                    }
                    self.close("if", "", pos)?;
                    SK::If { arms, els }
                }
                "while" => {
                    self.advance();
                    let cond = self.parse_expr()?;
                    let invs = self.parse_invariants()?;
                    let dec = if self.accept("decreases") { Some(self.parse_expr()?) } else { None };
                    self.expect("do", "after the while condition")?;
                    let body = self.parse_block()?;
                    self.close("while", "", pos)?;
                    SK::While { cond, invs, dec, body }
                }
                "for" => {
                    self.advance();
                    let (var, _) = self.ident("for the loop variable")?;
                    self.expect("in", "after the loop variable")?;
                    let first = self.parse_expr()?;
                    let range_hi = if self.accept("..") { Some(self.parse_expr()?) } else { None };
                    let invs = self.parse_invariants()?;
                    self.expect("do", "to begin the loop body")?;
                    let body = self.parse_block()?;
                    self.close("for", "", pos)?;
                    match range_hi {
                        Some(hi) => SK::ForRange { var, lo: first, hi, invs, body },
                        None => SK::ForEach { var, list: first, invs, body },
                    }
                }
                "match" => {
                    self.advance();
                    let scrut = self.parse_expr()?;
                    let mut cases = Vec::new();
                    while self.is("case") {
                        let cpos = self.advance().pos;
                        let pat = self.parse_pattern()?;
                        self.expect("then", "after the pattern")?;
                        let body = self.parse_block()?;
                        cases.push(Case { pat, body, pos: cpos });
                    }
                    let els = if self.accept("else") { Some(self.parse_block()?) } else { None };
                    self.close("match", "", pos)?;
                    SK::Match { scrut, cases, els }
                }
                "assert" => {
                    self.advance();
                    SK::Assert(self.parse_expr()?)
                }
                "check" => {
                    self.advance();
                    SK::Check(self.parse_expr()?)
                }
                "break" => {
                    self.advance();
                    SK::Break
                }
                "continue" => {
                    self.advance();
                    SK::Continue
                }
                _ => self.expr_stmt()?,
            }
        } else {
            self.expr_stmt()?
        };
        Ok(Stmt { pos, kind })
    }

    fn expr_stmt(&mut self) -> PResult<SK> {
        let e = self.parse_expr()?;
        match e.kind {
            EK::Call(..) => Ok(SK::Expr(e)),
            _ => Err(Diag::new(e.pos, "only a call can be used as a statement")
                .note("use 'let' to bind a value or 'set' to reassign a var")),
        }
    }

    fn parse_pattern(&mut self) -> PResult<Pat> {
        let t = self.peek().clone();
        if t.kind == TK::Int {
            self.advance();
            return Ok(Pat::Int(t.int));
        }
        if t.kind == TK::Sym && t.text == "-" && self.peek_at(1).kind == TK::Int {
            self.advance();
            let n = self.advance();
            return Ok(Pat::Int(-n.int));
        }
        if t.kind == TK::Str {
            self.advance();
            return Ok(Pat::Str(t.text));
        }
        let (name, _) = self.ident("for a pattern")?;
        let mut binds = Vec::new();
        if self.accept("(") {
            while !self.is(")") {
                let (f, _) = self.label("for a field in the pattern")?;
                self.expect(":", "after the field name in the pattern")?;
                let (b, _) = self.ident("for the binder in the pattern")?;
                binds.push((f, b));
                if !self.accept(",") {
                    break;
                }
            }
            self.expect(")", "to close the pattern")?;
        }
        Ok(Pat::Variant { name, binds })
    }

    pub fn parse_expr(&mut self) -> PResult<Expr> {
        let mut left = self.parse_unary()?;
        let mut first: Option<String> = None;
        loop {
            let t = self.peek().clone();
            let is_op = (t.kind == TK::Sym || t.kind == TK::Keyword) && BINOPS.contains(&t.text.as_str());
            if !is_op {
                break;
            }
            let op = t.text.clone();
            if let Some(f) = &first {
                if *f != op {
                    return Err(Diag::new(
                        t.pos,
                        &format!("mixing '{}' and '{}' requires parentheses", f, op),
                    )
                    .note("Vow has no operator precedence"));
                }
                if !CHAINABLE.contains(&op.as_str()) {
                    return Err(Diag::new(t.pos, &format!("'{}' cannot be chained; add parentheses", op)));
                }
            }
            self.advance();
            let right = self.parse_unary()?;
            let pos = left.pos;
            left = self.mk(pos, EK::Binary(op.clone(), Box::new(left), Box::new(right)));
            first = Some(op);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        let t = self.peek().clone();
        if (t.kind == TK::Sym && t.text == "-") || (t.kind == TK::Keyword && t.text == "not") {
            self.advance();
            let e = self.parse_unary()?;
            return Ok(self.mk(t.pos, EK::Unary(t.text, Box::new(e))));
        }
        self.parse_postfix()
    }

    fn parse_args(&mut self) -> PResult<Vec<Arg>> {
        let mut args = Vec::new();
        while !self.is(")") {
            let t = self.peek().clone();
            let named = (t.kind == TK::Ident || t.kind == TK::Keyword)
                && self.peek_at(1).kind == TK::Sym
                && self.peek_at(1).text == ":";
            let name = if named {
                self.advance();
                self.advance();
                Some(t.text.clone())
            } else {
                None
            };
            let inout = self.accept("inout");
            let value = if inout {
                let (v, vp) = self.ident("after 'inout'")?;
                self.mk(vp, EK::Var(v))
            } else {
                self.parse_expr()?
            };
            args.push(Arg { name, inout, value, pos: t.pos });
            if !self.accept(",") {
                break;
            }
        }
        self.expect(")", "to close the argument list")?;
        Ok(args)
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            if self.accept("(") {
                let args = self.parse_args()?;
                let pos = e.pos;
                e = self.mk(pos, EK::Call(Box::new(e), args));
            } else if self.accept("[") {
                let idx = self.parse_expr()?;
                self.expect("]", "to close the index")?;
                let pos = e.pos;
                e = self.mk(pos, EK::Index(Box::new(e), Box::new(idx)));
            } else if self.is(".") && (self.peek_at(1).kind == TK::Ident || self.peek_at(1).kind == TK::Keyword) {
                self.advance();
                let (f, _) = self.label("after '.'")?;
                let pos = e.pos;
                e = self.mk(pos, EK::Field(Box::new(e), f));
            } else if self.accept("with") {
                self.expect("(", "after 'with'")?;
                let args = self.parse_args()?;
                for a in &args {
                    if a.name.is_none() {
                        return Err(Diag::new(a.pos, "fields in 'with' must be named"));
                    }
                }
                let pos = e.pos;
                e = self.mk(pos, EK::With(Box::new(e), args));
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        let t = self.advance();
        let pos = t.pos;
        let kind = match t.kind {
            TK::Int => EK::Int(t.int),
            TK::Str => EK::Str(t.text),
            TK::Hole => EK::Hole(t.text),
            TK::Ident => EK::Var(t.text),
            TK::Keyword => match t.text.as_str() {
                "true" => EK::Bool(true),
                "false" => EK::Bool(false),
                "it" => EK::It,
                "result" => EK::Result,
                "forall" | "exists" => {
                    let (var, _) = self.ident("for the quantified variable")?;
                    self.expect("in", "after the quantified variable")?;
                    let lo = self.parse_expr()?;
                    self.expect("..", "in the quantifier range")?;
                    let hi = self.parse_expr()?;
                    self.expect(":", "after the quantifier range")?;
                    let body = self.parse_expr()?;
                    EK::Quant(t.text.clone(), var, Box::new(lo), Box::new(hi), Box::new(body))
                }
                _ => return Err(Diag::new(pos, &format!("expected an expression, found {}", describe(&t)))),
            },
            TK::Sym => match t.text.as_str() {
                "(" => {
                    let e = self.parse_expr()?;
                    self.expect(")", "to close the parenthesis")?;
                    return Ok(e);
                }
                "[" => {
                    let mut items = Vec::new();
                    while !self.is("]") {
                        items.push(self.parse_expr()?);
                        if !self.accept(",") {
                            break;
                        }
                    }
                    self.expect("]", "to close the list")?;
                    EK::List(items)
                }
                "{" => {
                    self.expect("}", "(only the empty map literal '{}' exists)")?;
                    EK::EmptyMap
                }
                _ => return Err(Diag::new(pos, &format!("expected an expression, found {}", describe(&t)))),
            },
            TK::Eof => return Err(Diag::new(pos, "expected an expression, found end of file")),
        };
        Ok(self.mk(pos, kind))
    }
}
