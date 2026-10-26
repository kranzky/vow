use crate::diag::Pos;

pub type ExprId = usize;

#[derive(Clone, Debug)]
pub struct Expr {
    pub id: ExprId,
    pub pos: Pos,
    pub kind: EK,
}

#[derive(Clone, Debug)]
pub enum EK {
    Int(i64),
    Str(String),
    Bool(bool),
    Var(String),
    It,
    Result,
    Hole(String),
    Call(Box<Expr>, Vec<Arg>),
    List(Vec<Expr>),
    EmptyMap,
    Index(Box<Expr>, Box<Expr>),
    Field(Box<Expr>, String),
    With(Box<Expr>, Vec<Arg>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    /// "forall" or "exists", bound variable, range low, range high, body.
    Quant(String, String, Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub struct Arg {
    pub name: Option<String>,
    pub inout: bool,
    pub value: Expr,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct TypeExpr {
    pub name: String,
    pub args: Vec<TypeExpr>,
    pub refine: Option<Box<Expr>>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub inout: bool,
    pub ty: TypeExpr,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Field>,
    pub ret: Option<TypeExpr>,
    pub uses: Vec<(String, Pos)>,
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    pub decreases: Option<Expr>,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<Field>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Field>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<Variant>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub struct AliasDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub enum Item {
    Fn(FnDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Alias(AliasDecl),
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub pos: Pos,
    pub kind: SK,
}

#[derive(Clone, Debug)]
pub enum SK {
    Let { name: String, ty: Option<TypeExpr>, value: Expr, mutable: bool },
    Set { name: String, value: Expr },
    Return(Option<Expr>),
    If { arms: Vec<(Expr, Vec<Stmt>)>, els: Option<Vec<Stmt>> },
    While { cond: Expr, invs: Vec<Expr>, dec: Option<Expr>, body: Vec<Stmt> },
    ForRange { var: String, lo: Expr, hi: Expr, invs: Vec<Expr>, body: Vec<Stmt> },
    ForEach { var: String, list: Expr, invs: Vec<Expr>, body: Vec<Stmt> },
    Match { scrut: Expr, cases: Vec<Case>, els: Option<Vec<Stmt>> },
    Assert(Expr),
    Check(Expr),
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub struct Case {
    pub pat: Pat,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Clone, Debug)]
pub enum Pat {
    Variant { name: String, binds: Vec<(String, String)> },
    Int(i64),
    Str(String),
}
