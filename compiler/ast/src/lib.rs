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

/// Offset every source span in a parsed program by `delta`.
///
/// File imports are merged into one semantic program. Giving each imported
/// file a unique span range keeps span-keyed type and lowering tables from
/// confusing nodes that happen to occupy the same byte offsets in different
/// files.
pub fn offset_program(program: &mut Program, delta: usize) {
    program.span = offset_span(program.span, delta);
    for global in &mut program.globals {
        global.span = offset_span(global.span, delta);
        if let Some(ty) = &mut global.ty {
            offset_type_expr(ty, delta);
        }
        offset_expr(&mut global.init, delta);
    }
    for usage in &mut program.uses {
        usage.span = offset_span(usage.span, delta);
    }
    for item in &mut program.structs {
        item.span = offset_span(item.span, delta);
        for field in &mut item.fields {
            field.span = offset_span(field.span, delta);
            offset_type_expr(&mut field.ty, delta);
        }
    }
    for item in &mut program.enums {
        item.span = offset_span(item.span, delta);
        for variant in &mut item.variants {
            variant.span = offset_span(variant.span, delta);
        }
    }
    for item in &mut program.traits {
        item.span = offset_span(item.span, delta);
        for method in &mut item.methods {
            method.span = offset_span(method.span, delta);
            offset_params(&mut method.params, delta);
            if let Some(ret) = &mut method.ret {
                offset_type_expr(ret, delta);
            }
        }
    }
    for item in &mut program.impls {
        item.span = offset_span(item.span, delta);
        for method in &mut item.methods {
            offset_fn_decl(method, delta);
        }
    }
    for item in &mut program.type_aliases {
        item.span = offset_span(item.span, delta);
        offset_type_expr(&mut item.ty, delta);
    }
    for item in &mut program.consts {
        item.span = offset_span(item.span, delta);
        if let Some(ty) = &mut item.ty {
            offset_type_expr(ty, delta);
        }
        offset_expr(&mut item.init, delta);
    }
    for function in &mut program.functions {
        offset_fn_decl(function, delta);
    }
}

/// Offset every span in a function declaration by `delta` bytes.
///
/// Monomorphized copies of a generic function reuse the source spans of the
/// body they were cloned from. Offsetting the spans of each copy keeps the
/// span-keyed semantic side tables (`expr_types`, `mono_calls`, `mono_structs`,
/// `decl_types`, `field_reads`, ...) from colliding between instantiations.
pub fn offset_fn_decl(decl: &mut FnDecl, delta: usize) {
    decl.span = offset_span(decl.span, delta);
    offset_params(&mut decl.params, delta);
    if let Some(ret) = &mut decl.ret {
        offset_type_expr(ret, delta);
    }
    offset_block(&mut decl.body, delta);
}

fn offset_span(span: Span, delta: usize) -> Span {
    Span::new(
        span.start.saturating_add(delta),
        span.end.saturating_add(delta),
    )
}

fn offset_params(params: &mut [Param], delta: usize) {
    for param in params {
        param.span = offset_span(param.span, delta);
        offset_type_expr(&mut param.ty, delta);
    }
}

fn offset_type_expr(ty: &mut TypeExpr, delta: usize) {
    ty.span = offset_span(ty.span, delta);
    if let Some((element, _)) = &mut ty.array {
        offset_type_expr(element, delta);
    }
    if let Some(tuple) = &mut ty.tuple {
        for element in tuple {
            offset_type_expr(element, delta);
        }
    }
    for arg in &mut ty.generic_args {
        offset_type_expr(arg, delta);
    }
}

fn offset_block(block: &mut Block, delta: usize) {
    block.span = offset_span(block.span, delta);
    for statement in &mut block.stmts {
        offset_statement(statement, delta);
    }
}

fn offset_statement(statement: &mut Stmt, delta: usize) {
    statement.span = offset_span(statement.span, delta);
    match &mut statement.kind {
        StmtKind::Let { ty, init, .. } => {
            if let Some(ty) = ty {
                offset_type_expr(ty, delta);
            }
            offset_expr(init, delta);
        }
        StmtKind::Assign { value, .. } => offset_expr(value, delta),
        StmtKind::AssignMany { values, .. } => {
            for value in values {
                offset_expr(value, delta);
            }
        }
        StmtKind::AssignField { value, .. } => offset_expr(value, delta),
        StmtKind::AssignIndex { index, value, .. } => {
            offset_expr(index, delta);
            offset_expr(value, delta);
        }
        StmtKind::AssignDeref { place, value } => {
            offset_expr(place, delta);
            offset_expr(value, delta);
        }
        StmtKind::AssignFieldDeref {
            reference, value, ..
        } => {
            offset_expr(reference, delta);
            offset_expr(value, delta);
        }
        StmtKind::AssignIndexDeref {
            reference,
            index,
            value,
        } => {
            offset_expr(reference, delta);
            offset_expr(index, delta);
            offset_expr(value, delta);
        }
        StmtKind::If {
            branches,
            else_branch,
        } => {
            for (condition, body) in branches {
                offset_expr(condition, delta);
                offset_block(body, delta);
            }
            if let Some(body) = else_branch {
                offset_block(body, delta);
            }
        }
        StmtKind::While { cond, body } => {
            offset_expr(cond, delta);
            offset_block(body, delta);
        }
        StmtKind::For {
            start, end, body, ..
        } => {
            offset_expr(start, delta);
            offset_expr(end, delta);
            offset_block(body, delta);
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                offset_expr(value, delta)
            }
        }
        StmtKind::Match { scrutinee, arms } => {
            offset_expr(scrutinee, delta);
            for arm in arms {
                offset_arm(arm, delta);
            }
        }
        StmtKind::Unsafe(block) | StmtKind::Defer(block) => offset_block(block, delta),
        StmtKind::Expr(expr) => offset_expr(expr, delta),
        StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
    }
}

fn offset_arm(arm: &mut MatchArm, delta: usize) {
    arm.span = offset_span(arm.span, delta);
    if let Some(guard) = &mut arm.guard {
        offset_expr(guard, delta);
    }
    offset_block(&mut arm.body, delta);
}

fn offset_expr(expr: &mut Expr, delta: usize) {
    expr.span = offset_span(expr.span, delta);
    match &mut expr.kind {
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Error => {}
        ExprKind::Unary { operand, .. } => offset_expr(operand, delta),
        ExprKind::Binary { left, right, .. } => {
            offset_expr(left, delta);
            offset_expr(right, delta);
        }
        ExprKind::Call {
            callee_span, args, ..
        } => {
            *callee_span = offset_span(*callee_span, delta);
            for arg in args {
                offset_expr(arg, delta);
            }
        }
        ExprKind::Cast { expr, ty } => {
            offset_expr(expr, delta);
            offset_type_expr(ty, delta);
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                offset_expr(item, delta);
            }
        }
        ExprKind::Index { base, index } => {
            offset_expr(base, delta);
            offset_expr(index, delta);
        }
        ExprKind::Field {
            base, name_span, ..
        } => {
            offset_expr(base, delta);
            *name_span = offset_span(*name_span, delta);
        }
        ExprKind::Try(inner) => offset_expr(inner, delta),
        ExprKind::Borrow { place, .. } => offset_expr(place, delta),
        ExprKind::Deref(inner) => offset_expr(inner, delta),
    }
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
    pub generic_args: Vec<TypeExpr>,
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
    /// Assignment through a reference: `*r = value`.
    AssignDeref {
        place: Box<Expr>,
        value: Expr,
    },
    /// Field assignment through a reference: `(*r).field = value`.
    AssignFieldDeref {
        reference: Box<Expr>,
        path: Vec<String>,
        value: Expr,
    },
    /// Element assignment through a reference: `(*r)[i] = value`.
    AssignIndexDeref {
        reference: Box<Expr>,
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
    /// `unsafe:` block. NEX treats the body like a normal block; the
    /// keyword documents intent and is reserved for future checks.
    Unsafe(Block),
    /// `defer:` block. The body runs at function exit.
    Defer(Block),
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
    /// Error propagation: `expr?`.
    Try(Box<Expr>),
    /// Shared (`&place`) or mutable (`&mut place`) borrow of a place expression.
    Borrow {
        mutable: bool,
        place: Box<Expr>,
    },
    /// Dereference of a reference: `*r`.
    Deref(Box<Expr>),
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
