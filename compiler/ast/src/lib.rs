//! Syntax tree produced by the NEX parser.
//!
//! The tree deliberately keeps source spans and no type information. Semantic
//! analysis stores inferred expression types in a side table keyed by span, so
//! the parser stays independent from the type checker.

use nex_diagnostics::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub modules: Vec<String>,
    pub globals: Vec<GlobalDecl>,
    pub uses: Vec<UseDecl>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub type_aliases: Vec<TypeAliasDecl>,
    pub consts: Vec<ConstDecl>,
    pub functions: Vec<FnDecl>,
    pub span: Span,
}

/// A trait declares method signatures. NEX uses static dispatch: an `impl`
/// block supplies a concrete method for one struct type.
/// `type Name = Type` alias declaration.
#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TraitDecl {
    pub name: String,
    pub public: bool,
    pub methods: Vec<TraitMethodDecl>,
    pub span: Span,
}

/// A method signature inside a trait. `self` is implicit and typed by the
/// implementing struct, so it is not part of `params`.
#[derive(Debug, Clone)]
pub struct TraitMethodDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub span: Span,
}

/// `impl Trait for Type:` with concrete method bodies. Method `FnDecl`s keep
/// their plain names; semantic analysis registers them as `Type.method`.
#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub trait_name: String,
    pub type_name: String,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

/// A module-level `var` binding. Globals are shared by every function, so
/// threads can use them (with a mutex) for shared state.
#[derive(Debug, Clone)]
pub struct GlobalDecl {
    pub name: String,
    pub mutable: bool,
    pub ty: Option<TypeExpr>,
    pub init: Expr,
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
    pub type_params: Vec<String>,
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
    pub type_params: Vec<String>,
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
    /// `Some((element, Some(len)))` is `[T; N]`; `Some((element, None))` is
    /// the growable form `[T]`.
    pub array: Option<(Box<TypeExpr>, Option<usize>)>,
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
        /// Names bound by a tuple destructuring pattern (`let (a, b) = ...`).
        names: Option<Vec<String>>,
        mutable: bool,
        ty: Option<TypeExpr>,
        init: Expr,
    },
    Assign {
        name: String,
        value: Expr,
    },
    /// Parallel assignment (`a, b = b, a`).
    AssignMany {
        names: Vec<String>,
        values: Vec<Expr>,
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
    /// `pass`: an intentionally empty block.
    Pass,
    /// `unsafe:` block. NEX 0.2 treats the body like a normal block; the
    /// keyword documents intent and is reserved for future checks.
    Unsafe(Block),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    /// `Pattern::Else` is the fallback arm.
    pub pattern: Pattern,
    /// Optional guard: the arm matches only when this expression is true.
    pub guard: Option<Expr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Else,
    Variant(String),
    Int(i64),
    /// Inclusive integer range (`case 1..5` matches 1 through 5).
    Range(i64, i64),
    Bool(bool),
    Char(char),
    Str(String),
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
    BitNot,
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
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
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
            Self::BitAnd => "&",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::Shl => "<<",
            Self::Shr => ">>",
        }
    }
}
