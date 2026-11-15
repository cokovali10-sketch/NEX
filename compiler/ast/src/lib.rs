//! Syntax tree produced by the NEX parser.
//!
//! The tree deliberately keeps source spans and no type information. Semantic
//! analysis stores inferred expression types in a side table keyed by span, so
//! the parser stays independent from the type checker.

use nex_diagnostics::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub modules: Vec<String>,
    pub uses: Vec<UseDecl>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub consts: Vec<ConstDecl>,
    pub functions: Vec<FnDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct UseDecl {
    pub path: Vec<String>,
    pub file: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub public: bool,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub public: bool,
    pub variants: Vec<VariantDecl>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct VariantDecl {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    pub public: bool,
    pub ty: Option<TypeExpr>,
    pub init: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name: String,
    pub public: bool,
    pub extern_abi: Option<String>,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeExpr {
    pub name: String,
    pub array: Option<(Box<TypeExpr>, usize)>,
    pub tuple: Option<Vec<TypeExpr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let {
        name: String,
        mutable: bool,
        ty: Option<TypeExpr>,
        init: Expr,
    },
    Assign {
        name: String,
        value: Expr,
    },
    AssignField {
        base: String,
        path: Vec<String>,
        value: Expr,
    },
    AssignIndex {
        base: String,
        index: Expr,
        value: Expr,
    },
    If {
        branches: Vec<(Expr, Block)>,
        else_branch: Option<Block>,
    },
    While {
        cond: Expr,
        body: Block,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        body: Block,
    },
    Return(Option<Expr>),
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    /// `None` is the fallback (`else`) arm.
    pub pattern: Option<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
    Ident(String),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        callee: String,
        callee_span: Span,
        args: Vec<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        ty: TypeExpr,
    },
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Field {
        base: Box<Expr>,
        name: String,
        name_span: Span,
    },
    /// Parser recovery node. It must be rejected by semantic analysis.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub const fn is_comparison(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Ne | Self::Lt | Self::Le | Self::Gt | Self::Ge
        )
    }

    pub const fn is_arithmetic(self) -> bool {
        matches!(
            self,
            Self::Add | Self::Sub | Self::Mul | Self::Div | Self::Rem
        )
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::And => "and",
            Self::Or => "or",
        }
    }
}
