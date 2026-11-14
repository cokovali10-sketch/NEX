mod types;

pub use types::Type;

use nex_ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use nex_diagnostics::{Diagnostic, Span};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone)]
pub struct FnSig {
    pub name: String,
    pub params: Vec<Type>,
    pub ret: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    /// Interned copy of `name`, used by [`Type::Struct`].
    pub name_static: &'static str,
    pub fields: Vec<(String, Type)>,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub name_static: &'static str,
    pub variants: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ArrayInfo {
    pub display: &'static str,
    pub llvm: &'static str,
    pub elem: Type,
    pub len: usize,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub structs: BTreeMap<String, StructInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    pub arrays: BTreeMap<String, ArrayInfo>,
    /// Variant name to the interned name of the enum that declares it.
    pub variant_map: HashMap<String, &'static str>,
    pub consts: BTreeMap<String, Type>,
    pub const_exprs: BTreeMap<String, Expr>,
    pub functions: BTreeMap<String, FnSig>,
    pub expr_types: HashMap<Span, Type>,
    pub decl_types: HashMap<Span, Type>,
    /// Field reads keyed by expression span: (struct name, field index, field type).
    pub field_reads: HashMap<Span, (&'static str, u32, Type)>,
    /// Field writes keyed by statement span: (struct name, field index, field type).
    pub field_writes: HashMap<Span, (&'static str, u32, Type)>,
    /// Enum variant values keyed by expression span: (enum name, variant index).
    pub variant_values: HashMap<Span, (&'static str, u32)>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    pub fn function(&self, name: &str) -> Option<&FnSig> {
        self.functions.get(name)
    }

    pub fn struct_info(&self, name: &str) -> Option<&StructInfo> {
        self.structs.get(name)
    }

    pub fn enum_info(&self, name: &str) -> Option<&EnumInfo> {
        self.enums.get(name)
    }

    pub fn array_info(&self, name: &str) -> Option<&ArrayInfo> {
        self.arrays.get(name)
    }

    pub fn field_read(&self, span: Span) -> Option<(&'static str, u32, Type)> {
        self.field_reads.get(&span).copied()
    }

    pub fn field_write(&self, span: Span) -> Option<(&'static str, u32, Type)> {
        self.field_writes.get(&span).copied()
    }

    pub fn variant_value(&self, span: Span) -> Option<(&'static str, u32)> {
        self.variant_values.get(&span).copied()
    }

    pub fn expr_type(&self, span: Span) -> Option<Type> {
        self.expr_types.get(&span).copied()
    }

    pub fn decl_type(&self, span: Span) -> Option<Type> {
        self.decl_types.get(&span).copied()
    }
}

pub use types::Type as NexType;

#[derive(Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
}

struct Checker<'a> {
    program: &'a Program,
    analysis: Analysis,
    scopes: Vec<HashMap<String, VarInfo>>,
    current_fn: Option<FnSig>,
    loop_depth: usize,
}

pub fn analyze(program: &Program) -> Analysis {
    Checker::analyze_program(program)
}

impl<'a> Checker<'a> {
    fn analyze_program(program: &'a Program) -> Analysis {
        let mut checker = Self {
            program,
            analysis: Analysis {
                structs: BTreeMap::new(),
                enums: BTreeMap::new(),
                arrays: BTreeMap::new(),
                variant_map: HashMap::new(),
                consts: BTreeMap::new(),
                const_exprs: BTreeMap::new(),
                functions: BTreeMap::new(),
                expr_types: HashMap::new(),
                decl_types: HashMap::new(),
                field_reads: HashMap::new(),
                field_writes: HashMap::new(),
                variant_values: HashMap::new(),
                diagnostics: Vec::new(),
            },
            scopes: Vec::new(),
            current_fn: None,
            loop_depth: 0,
        };
        checker.collect_structs();
        checker.collect_enums();
        checker.collect_consts();
        checker.collect_functions();
        checker.check_main();
        checker.check_functions();
        checker.analysis
    }

    fn collect_functions(&mut self) {
        for function in &self.program.functions {
            if function.name == "print" || function.name == "println" {
                self.error(
                    "NXS019",
                    format!(
                        "function name `{}` is reserved for the built-in print family",
                        function.name
                    ),
                    function.span,
                );
            }

            if function.name.starts_with("nex_rt_") {
                self.error(
                    "NXS020",
                    format!(
                        "function name `{}` uses the reserved `nex_rt_` ABI prefix",
                        function.name
                    ),
                    function.span,
                );
            }

            if self.analysis.structs.contains_key(&function.name) {
                self.error(
                    "NXS003",
                    format!("name `{}` is already declared as a struct", function.name),
                    function.span,
                );
                continue;
            }

            if self.analysis.functions.contains_key(&function.name) {
                self.error(
                    "NXS003",
                    format!("duplicate function `{}`", function.name),
                    function.span,
                );
                continue;
            }

            let mut params = Vec::with_capacity(function.params.len());
            for param in &function.params {
                let ty = self.resolve_type(&param.ty);
                if ty == Type::Void {
                    self.error(
                        "NXS002",
                        format!("parameter `{}` cannot have type `void`", param.name),
                        param.ty.span,
                    );
                }
                if matches!(ty, Type::Struct(_)) {
                    self.error(
                        "NXS028",
                        format!(
                            "parameter `{}` has struct type `{ty}`; passing struct values to functions is planned for a later release",
                            param.name
                        ),
                        param.ty.span,
                    );
                }
                params.push(ty);
            }

            let ret = function
                .ret
                .as_ref()
                .map(|ty| self.resolve_type(ty))
                .unwrap_or(Type::Void);

            if matches!(ret, Type::Struct(_)) {
                self.error(
                    "NXS028",
                    format!(
                        "function `{}` returns struct type `{ret}`; returning struct values is planned for a later release",
                        function.name
                    ),
                    function.span,
                );
            }

            self.analysis.functions.insert(
                function.name.clone(),
                FnSig {
                    name: function.name.clone(),
                    params,
                    ret,
                    span: function.span,
                },
            );
        }
    }

    fn collect_enums(&mut self) {
        for definition in &self.program.enums {
            if self.analysis.enums.contains_key(&definition.name) {
                self.error(
                    "NXS003",
                    format!("duplicate enum `{}`", definition.name),
                    definition.span,
                );
                continue;
            }

            if definition.variants.is_empty() {
                self.error(
                    "NXS029",
                    format!(
                        "enum `{}` must declare at least one variant",
                        definition.name
                    ),
                    definition.span,
                );
                continue;
            }

            let mut variants = Vec::with_capacity(definition.variants.len());
            let mut seen = std::collections::HashSet::new();
            for variant in &definition.variants {
                if !seen.insert(variant.name.clone()) {
                    self.error(
                        "NXS003",
                        format!(
                            "duplicate variant `{}` in enum `{}`",
                            variant.name, definition.name
                        ),
                        variant.span,
                    );
                    continue;
                }
                if self.analysis.variant_map.contains_key(&variant.name) {
                    self.error(
                        "NXS029",
                        format!(
                            "variant name `{}` is already used by another enum",
                            variant.name
                        ),
                        variant.span,
                    );
                    continue;
                }
                variants.push(variant.name.clone());
            }

            if variants.is_empty() {
                self.error(
                    "NXS029",
                    format!(
                        "enum `{}` must declare at least one variant",
                        definition.name
                    ),
                    definition.span,
                );
                continue;
            }

            let name_static: &'static str = Box::leak(definition.name.clone().into_boxed_str());
            for variant in &variants {
                self.analysis
                    .variant_map
                    .insert(variant.clone(), name_static);
            }
            self.analysis.enums.insert(
                definition.name.clone(),
                EnumInfo {
                    name: definition.name.clone(),
                    name_static,
                    variants,
                },
            );
        }
    }

    fn collect_structs(&mut self) {
        for definition in &self.program.structs {
            if self.analysis.structs.contains_key(&definition.name) {
                self.error(
                    "NXS003",
                    format!("duplicate struct `{}`", definition.name),
                    definition.span,
                );
                continue;
            }

            if definition.fields.is_empty() {
                self.error(
                    "NXS025",
                    format!(
                        "struct `{}` must declare at least one field",
                        definition.name
                    ),
                    definition.span,
                );
                continue;
            }

            let mut fields = Vec::with_capacity(definition.fields.len());
            let mut seen = std::collections::HashSet::new();
            for field in &definition.fields {
                if !seen.insert(field.name.clone()) {
                    self.error(
                        "NXS003",
                        format!(
                            "duplicate field `{}` in struct `{}`",
                            field.name, definition.name
                        ),
                        field.span,
                    );
                    continue;
                }

                let ty = self.resolve_type(&field.ty);
                if ty == Type::Error {
                    continue;
                }
                if ty == Type::Void {
                    self.error(
                        "NXS002",
                        format!("field `{}` cannot have type `void`", field.name),
                        field.ty.span,
                    );
                    continue;
                }
                if matches!(ty, Type::Struct(_)) {
                    self.error(
                        "NXS025",
                        format!(
                            "field `{}` has struct type `{ty}`; nested struct fields are planned for a later release",
                            field.name
                        ),
                        field.ty.span,
                    );
                    continue;
                }
                fields.push((field.name.clone(), ty));
            }

            let name_static: &'static str = Box::leak(definition.name.clone().into_boxed_str());
            self.analysis.structs.insert(
                definition.name.clone(),
                StructInfo {
                    name: definition.name.clone(),
                    name_static,
                    fields,
                },
            );
        }
    }

    fn collect_consts(&mut self) {
        for constant in &self.program.consts {
            if self.analysis.consts.contains_key(&constant.name) {
                self.error(
                    "NXS003",
                    format!("duplicate constant `{}`", constant.name),
                    constant.span,
                );
                continue;
            }

            if !self.is_const_expr(&constant.init, &mut std::collections::HashSet::new()) {
                self.error(
                    "NXS023",
                    format!(
                        "constant `{}` initializer must be a compile-time constant expression",
                        constant.name
                    ),
                    constant.init.span,
                );
            }

            let expected = constant.ty.as_ref().map(|ty| self.resolve_type(ty));
            let inferred = if let Some(expected) = expected {
                self.check_expected(&constant.init, expected)
            } else {
                self.infer(&constant.init)
            };

            let declared_ty = expected.unwrap_or(inferred);
            if declared_ty == Type::Void || declared_ty == Type::Error {
                if declared_ty == Type::Void {
                    self.error(
                        "NXS002",
                        format!("constant `{}` cannot have type `void`", constant.name),
                        constant.span,
                    );
                }
                continue;
            }

            if matches!(declared_ty, Type::Struct(_)) {
                self.error(
                    "NXS028",
                    format!(
                        "constant `{}` has struct type `{declared_ty}`; struct constants are planned for a later release",
                        constant.name
                    ),
                    constant.span,
                );
                continue;
            }

            self.analysis
                .consts
                .insert(constant.name.clone(), declared_ty);
            self.analysis
                .const_exprs
                .insert(constant.name.clone(), constant.init.clone());
        }
    }

    fn is_const_expr(&self, expr: &Expr, visiting: &mut std::collections::HashSet<String>) -> bool {
        match &expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Char(_)
            | ExprKind::Str(_) => true,
            ExprKind::Unary { operand, .. } => self.is_const_expr(operand, visiting),
            ExprKind::Binary { left, right, .. } => {
                self.is_const_expr(left, visiting) && self.is_const_expr(right, visiting)
            }
            ExprKind::Cast { expr: inner, .. } => self.is_const_expr(inner, visiting),
            ExprKind::Ident(name) => {
                if !self.analysis.consts.contains_key(name) {
                    return false;
                }
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let result = self
                    .analysis
                    .const_exprs
                    .get(name)
                    .map(|init| self.is_const_expr(init, visiting))
                    .unwrap_or(false);
                visiting.remove(name);
                result
            }
            _ => false,
        }
    }

    fn check_main(&mut self) {
        let sig = self.analysis.functions.get("main").cloned();
        let Some(sig) = sig else {
            self.error(
                "NXS011",
                "program entry point `fn main()` is required",
                self.program.span,
            );
            return;
        };

        if !sig.params.is_empty() {
            self.error(
                "NXS011",
                "NEX 0.1 `main` must not accept parameters",
                sig.span,
            );
        }

        if !matches!(sig.ret, Type::Void | Type::I32) {
            self.error(
                "NXS011",
                format!(
                    "NEX 0.1 `main` must return `void` or `i32`, found `{}`",
                    sig.ret
                ),
                sig.span,
            );
        }
    }

    fn check_functions(&mut self) {
        let functions = self.program.functions.clone();
        for function in functions {
            let Some(sig) = self.analysis.functions.get(&function.name).cloned() else {
                continue;
            };
            self.current_fn = Some(sig.clone());
            self.scopes.clear();
            let mut param_scope = HashMap::new();
            for (param, ty) in function.params.iter().zip(sig.params.iter()) {
                if param_scope.contains_key(&param.name) {
                    self.error(
                        "NXS018",
                        format!("duplicate parameter `{}`", param.name),
                        param.span,
                    );
                }
                param_scope.insert(
                    param.name.clone(),
                    VarInfo {
                        ty: *ty,
                        mutable: false,
                    },
                );
            }
            self.scopes.push(param_scope);
            self.loop_depth = 0;
            self.check_block(&function.body, true);

            if sig.ret != Type::Void && !self.block_returns(&function.body) {
                self.error(
                    "NXS010",
                    format!(
                        "function `{}` may exit without returning a value of type `{}`",
                        function.name, sig.ret
                    ),
                    function.span,
                );
            }

            self.current_fn = None;
            self.scopes.clear();
        }
    }

    fn check_block(&mut self, block: &nex_ast::Block, _top_level: bool) {
        self.scopes.push(HashMap::new());
        let mut returned = false;
        for statement in &block.stmts {
            if returned {
                self.warning("NXS017", "unreachable statement", statement.span);
            }
            self.check_statement(statement);
            returned = returned || self.stmt_returns(statement);
        }
        self.scopes.pop();
    }

    fn check_statement(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Let {
                name,
                mutable,
                ty,
                init,
            } => {
                let expected = ty
                    .as_ref()
                    .map(|annotation| self.resolve_type(annotation))
                    .unwrap_or(Type::Error);

                let inferred = if ty.is_some() {
                    self.check_expected(init, expected)
                } else {
                    self.infer(init)
                };

                let declared_ty = if ty.is_some() {
                    if inferred == Type::Error {
                        Type::Error
                    } else {
                        expected
                    }
                } else {
                    inferred
                };

                if declared_ty == Type::Void {
                    self.error(
                        "NXS002",
                        format!("variable `{name}` cannot have type `void`"),
                        statement.span,
                    );
                }

                self.analysis.decl_types.insert(statement.span, declared_ty);
                self.declare(name, declared_ty, *mutable, statement.span);
            }
            StmtKind::Assign { name, value } => {
                let Some(var) = self.lookup(name) else {
                    self.error(
                        "NXS016",
                        format!("cannot assign to undeclared variable `{name}`"),
                        statement.span,
                    );
                    self.infer(value);
                    return;
                };
                if !var.mutable {
                    self.error(
                        "NXS007",
                        format!(
                            "cannot assign to immutable variable `{name}`; declare it with `var`"
                        ),
                        statement.span,
                    );
                }
                let var_ty = var.ty;
                self.check_expected(value, var_ty);
            }
            StmtKind::AssignField { base, field, value } => {
                let Some(var) = self.lookup(base) else {
                    self.error(
                        "NXS016",
                        format!("cannot assign to a field of undeclared variable `{base}`"),
                        statement.span,
                    );
                    self.infer(value);
                    return;
                };
                if !var.mutable {
                    self.error(
                        "NXS007",
                        format!(
                            "cannot assign to a field of immutable variable `{base}`; declare it with `var`"
                        ),
                        statement.span,
                    );
                }
                let var_ty = var.ty;
                if let Type::Struct(struct_name) = var_ty {
                    let info = self.analysis.structs.get(struct_name).cloned();
                    if let Some(info) = info {
                        if let Some(index) = info.fields.iter().position(|(name, _)| name == field)
                        {
                            let field_ty = info.fields[index].1;
                            self.analysis
                                .field_writes
                                .insert(statement.span, (info.name_static, index as u32, field_ty));
                            self.check_expected(value, field_ty);
                        } else {
                            self.error(
                                "NXS027",
                                format!("struct `{struct_name}` has no field `{field}`"),
                                statement.span,
                            );
                            self.infer(value);
                        }
                    } else {
                        self.infer(value);
                    }
                } else {
                    self.error(
                        "NXS027",
                        format!("cannot assign field `{field}` on a value of type `{var_ty}`"),
                        statement.span,
                    );
                    self.infer(value);
                }
            }
            StmtKind::If {
                branches,
                else_branch,
            } => {
                for (condition, block) in branches {
                    self.check_bool(condition);
                    self.check_block(block, false);
                }
                if let Some(block) = else_branch {
                    self.check_block(block, false);
                }
            }
            StmtKind::While { cond, body } => {
                self.check_bool(cond);
                self.loop_depth += 1;
                self.check_block(body, false);
                self.loop_depth -= 1;
            }
            StmtKind::For {
                var,
                start,
                end,
                body,
            } => {
                let start_ty = self.infer(start);
                let end_ty = self.infer(end);
                if start_ty != Type::Error && end_ty != Type::Error {
                    if !start_ty.is_integer() || !end_ty.is_integer() {
                        self.error("NXS024", "for loop bounds must be integers", statement.span);
                    } else if start_ty != end_ty {
                        self.error(
                            "NXS008",
                            format!(
                                "for loop bounds have different types `{start_ty}` and `{end_ty}`"
                            ),
                            statement.span,
                        );
                    }
                }

                let var_ty = if start_ty != Type::Error {
                    start_ty
                } else {
                    end_ty
                };

                self.loop_depth += 1;
                self.scopes.push(HashMap::new());
                if var_ty != Type::Error {
                    self.declare(var, var_ty, false, statement.span);
                }
                self.check_block(body, false);
                self.scopes.pop();
                self.loop_depth -= 1;
            }
            StmtKind::AssignIndex { base, index, value } => {
                let Some(var) = self.lookup(base) else {
                    self.error(
                        "NXS016",
                        format!("cannot assign to an element of undeclared variable `{base}`"),
                        statement.span,
                    );
                    self.infer(index);
                    self.infer(value);
                    return;
                };
                if !var.mutable {
                    self.error(
                        "NXS007",
                        format!(
                            "cannot assign to an element of immutable variable `{base}`; declare it with `var`"
                        ),
                        statement.span,
                    );
                }
                let var_ty = var.ty;
                if let Type::Array { display, .. } = var_ty {
                    let info = self.analysis.arrays.get(display).cloned();
                    if let Some(info) = info {
                        let index_ty = self.infer(index);
                        if index_ty != Type::Error && !index_ty.is_integer() {
                            self.error(
                                "NXS036",
                                format!("array index must be an integer, found `{index_ty}`"),
                                index.span,
                            );
                        }
                        self.check_expected(value, info.elem);
                    } else {
                        self.infer(index);
                        self.infer(value);
                    }
                } else {
                    self.error(
                        "NXS036",
                        format!("cannot index a value of type `{var_ty}`"),
                        statement.span,
                    );
                    self.infer(index);
                    self.infer(value);
                }
            }
            StmtKind::Match { scrutinee, arms } => {
                let scrut_ty = self.infer(scrutinee);
                let enum_name = match scrut_ty {
                    Type::Enum(name) => Some(name),
                    Type::Error => None,
                    other => {
                        self.error(
                            "NXS031",
                            format!("match scrutinee must have an enum type, found `{other}`"),
                            statement.span,
                        );
                        None
                    }
                };

                if let Some(enum_name) = enum_name {
                    let info = self.analysis.enums.get(enum_name).cloned();
                    if let Some(info) = info {
                        let mut seen = std::collections::HashSet::new();
                        let mut else_seen = false;
                        let mut else_not_last = false;
                        for (index, arm) in arms.iter().enumerate() {
                            match &arm.pattern {
                                Some(variant) => {
                                    if else_seen {
                                        self.error(
                                            "NXS034",
                                            "case arm cannot follow an `else` arm".to_string(),
                                            arm.span,
                                        );
                                    }
                                    if !seen.insert(variant.clone()) {
                                        self.error(
                                            "NXS032",
                                            format!("duplicate match arm for variant `{variant}`"),
                                            arm.span,
                                        );
                                    }
                                    if !info.variants.iter().any(|value| value == variant) {
                                        self.error(
                                            "NXS030",
                                            format!(
                                                "enum `{enum_name}` has no variant `{variant}`"
                                            ),
                                            arm.span,
                                        );
                                    }
                                }
                                None => {
                                    if else_seen {
                                        self.error(
                                            "NXS034",
                                            "only one `else` arm is allowed".to_string(),
                                            arm.span,
                                        );
                                    }
                                    else_seen = true;
                                    if index + 1 != arms.len() {
                                        else_not_last = true;
                                    }
                                }
                            }
                            self.check_block(&arm.body, false);
                        }
                        if else_not_last {
                            self.error(
                                "NXS034",
                                "`else` arm must be the last match arm".to_string(),
                                statement.span,
                            );
                        }
                        if !else_seen && seen.len() < info.variants.len() {
                            self.warning(
                                "NXS033",
                                format!("match does not cover all variants of enum `{enum_name}`"),
                                statement.span,
                            );
                        }
                    }
                } else {
                    for arm in arms {
                        self.check_block(&arm.body, false);
                    }
                }
            }
            StmtKind::Return(value) => {
                let Some(sig) = self.current_fn.clone() else {
                    return;
                };
                if sig.ret == Type::Void {
                    if let Some(value) = value {
                        self.infer(value);
                        self.error("NXS009", "function does not return a value", statement.span);
                    }
                } else if let Some(value) = value {
                    self.check_expected(value, sig.ret);
                } else {
                    self.error(
                        "NXS009",
                        format!("function must return a value of type `{}`", sig.ret),
                        statement.span,
                    );
                }
            }
            StmtKind::Break | StmtKind::Continue => {
                if self.loop_depth == 0 {
                    let keyword = match statement.kind {
                        StmtKind::Break => "break",
                        StmtKind::Continue => "continue",
                        _ => unreachable!(),
                    };
                    self.error(
                        "NXS012",
                        format!("`{keyword}` outside of a loop"),
                        statement.span,
                    );
                }
            }
            StmtKind::Expr(expr) => {
                self.infer(expr);
            }
        }
    }

    fn check_bool(&mut self, expr: &Expr) {
        let ty = self.infer(expr);
        if ty != Type::Error && ty != Type::Bool {
            self.error(
                "NXS014",
                format!("condition must be `bool`, found `{ty}`"),
                expr.span,
            );
        }
    }

    fn check_expected(&mut self, expr: &Expr, expected: Type) -> Type {
        if expected == Type::Error {
            return Type::Error;
        }

        if let ExprKind::Int(_) = &expr.kind {
            if let Some(range) = expected.integer_range() {
                let value = match expr.kind {
                    ExprKind::Int(value) => value as i128,
                    _ => 0,
                };
                if value < range.0 || value > range.1 {
                    self.error(
                        "NXS021",
                        format!("integer literal {} does not fit in `{}`", value, expected),
                        expr.span,
                    );
                    self.analysis.expr_types.insert(expr.span, Type::Error);
                    return Type::Error;
                }
            }
        }

        if self.coerce_literal(expr, expected) {
            return expected;
        }

        let actual = self.infer(expr);
        if actual == Type::Error || expected == Type::Error {
            return Type::Error;
        }
        if actual != expected {
            self.error(
                "NXS006",
                format!("type mismatch: expected `{expected}`, found `{actual}`"),
                expr.span,
            );
            return Type::Error;
        }
        actual
    }

    fn infer(&mut self, expr: &Expr) -> Type {
        let ty = match &expr.kind {
            ExprKind::Int(_) => Type::I64,
            ExprKind::Float(_) => Type::F64,
            ExprKind::Str(_) => Type::String,
            ExprKind::Char(_) => Type::Char,
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Ident(name) => {
                if let Some(var) = self.lookup(name) {
                    var.ty
                } else if let Some(ty) = self.analysis.consts.get(name).copied() {
                    ty
                } else if let Some(enum_name) = self.analysis.variant_map.get(name).copied() {
                    let info = self.analysis.enum_info(enum_name);
                    let index = info
                        .and_then(|info| info.variants.iter().position(|variant| variant == name))
                        .map(|index| index as u32);
                    if let Some(index) = index {
                        self.analysis
                            .variant_values
                            .insert(expr.span, (enum_name, index));
                    }
                    Type::Enum(enum_name)
                } else {
                    self.error("NXS001", format!("unknown variable `{name}`"), expr.span);
                    Type::Error
                }
            }
            ExprKind::Unary { op, operand } => {
                let operand_ty = self.infer(operand);
                if operand_ty == Type::Error {
                    Type::Error
                } else {
                    match op {
                        nex_ast::UnaryOp::Neg => {
                            if operand_ty.is_signed_integer() || operand_ty.is_float() {
                                operand_ty
                            } else if operand_ty.is_unsigned_integer() {
                                self.error(
                                    "NXS008",
                                    format!(
                                        "unary `-` is not defined for unsigned type `{operand_ty}`"
                                    ),
                                    expr.span,
                                );
                                Type::Error
                            } else {
                                self.error(
                                    "NXS008",
                                    format!("unary `-` is not defined for type `{operand_ty}`"),
                                    expr.span,
                                );
                                Type::Error
                            }
                        }
                        nex_ast::UnaryOp::Not => {
                            if operand_ty == Type::Bool {
                                Type::Bool
                            } else {
                                self.error(
                                    "NXS008",
                                    format!("`not` requires `bool`, found `{operand_ty}`"),
                                    expr.span,
                                );
                                Type::Error
                            }
                        }
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                use nex_ast::BinOp;
                match op {
                    BinOp::And | BinOp::Or => {
                        self.check_bool(left);
                        self.check_bool(right);
                        Type::Bool
                    }
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                        let Some(ty) = self.unify_operands(left, right) else {
                            return Type::Error;
                        };
                        if ty.is_numeric() {
                            ty
                        } else {
                            self.error(
                                "NXS008",
                                format!("arithmetic is not defined for type `{ty}`"),
                                expr.span,
                            );
                            Type::Error
                        }
                    }
                    BinOp::Eq | BinOp::Ne => {
                        let Some(ty) = self.unify_operands(left, right) else {
                            return Type::Error;
                        };
                        if ty == Type::Bool
                            || ty == Type::Char
                            || ty == Type::String
                            || matches!(ty, Type::Enum(_))
                            || ty.is_numeric()
                        {
                            Type::Bool
                        } else {
                            self.error(
                                "NXS008",
                                format!("equality is not implemented for type `{ty}` in NEX 0.1"),
                                expr.span,
                            );
                            Type::Error
                        }
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        let Some(ty) = self.unify_operands(left, right) else {
                            return Type::Error;
                        };
                        if ty == Type::Bool
                            || ty == Type::Char
                            || ty == Type::String
                            || ty.is_numeric()
                        {
                            Type::Bool
                        } else {
                            self.error(
                                "NXS008",
                                format!("ordering is not defined for type `{ty}` in NEX 0.1"),
                                expr.span,
                            );
                            Type::Error
                        }
                    }
                }
            }
            ExprKind::Call {
                callee,
                callee_span,
                args,
            } => self.check_call(callee, *callee_span, args, expr.span),
            ExprKind::Field {
                base,
                name,
                name_span,
            } => {
                if let ExprKind::Ident(base_name) = &base.kind
                    && self.lookup(base_name).is_none()
                    && self.analysis.enum_info(base_name).is_some()
                {
                    let info = self.analysis.enum_info(base_name).cloned().unwrap();
                    if let Some(index) = info.variants.iter().position(|variant| variant == name) {
                        self.analysis
                            .variant_values
                            .insert(expr.span, (info.name_static, index as u32));
                        Type::Enum(info.name_static)
                    } else {
                        self.error(
                            "NXS030",
                            format!("enum `{base_name}` has no variant `{name}`"),
                            *name_span,
                        );
                        Type::Error
                    }
                } else {
                    let base_ty = self.infer(base);
                    if base_ty == Type::Error {
                        Type::Error
                    } else if let Type::Struct(struct_name) = base_ty {
                        let info = self.analysis.structs.get(struct_name).cloned();
                        if let Some(info) = info
                            && let Some(index) =
                                info.fields.iter().position(|(field, _)| field == name)
                        {
                            let field_ty = info.fields[index].1;
                            self.analysis
                                .field_reads
                                .insert(expr.span, (info.name_static, index as u32, field_ty));
                            field_ty
                        } else {
                            self.error(
                                "NXS027",
                                format!("struct `{struct_name}` has no field `{name}`"),
                                *name_span,
                            );
                            Type::Error
                        }
                    } else {
                        self.error(
                            "NXS027",
                            format!("cannot access field `{name}` on a value of type `{base_ty}`"),
                            *name_span,
                        );
                        Type::Error
                    }
                }
            }
            ExprKind::Array(elements) => {
                if elements.is_empty() {
                    self.error(
                        "NXS035",
                        "empty array literals are not supported in NEX 0.2".to_string(),
                        expr.span,
                    );
                    Type::Error
                } else {
                    let elem_ty = self.infer(&elements[0]);
                    if elem_ty == Type::Error {
                        for element in &elements[1..] {
                            self.infer(element);
                        }
                        Type::Error
                    } else {
                        for element in &elements[1..] {
                            self.check_expected(element, elem_ty);
                        }
                        let (display, llvm) = intern_array_type(elem_ty, elements.len());
                        if !self.analysis.arrays.contains_key(display) {
                            self.analysis.arrays.insert(
                                display.to_string(),
                                ArrayInfo {
                                    display,
                                    llvm,
                                    elem: elem_ty,
                                    len: elements.len(),
                                },
                            );
                        }
                        Type::Array { display, llvm }
                    }
                }
            }
            ExprKind::Index { base, index } => {
                let base_ty = self.infer(base);
                let index_ty = self.infer(index);
                if index_ty != Type::Error && !index_ty.is_integer() {
                    self.error(
                        "NXS036",
                        format!("array index must be an integer, found `{index_ty}`"),
                        index.span,
                    );
                }
                if base_ty == Type::Error {
                    Type::Error
                } else if let Type::Array { display, .. } = base_ty {
                    self.analysis
                        .arrays
                        .get(display)
                        .map(|info| info.elem)
                        .unwrap_or(Type::Error)
                } else {
                    self.error(
                        "NXS036",
                        format!("cannot index a value of type `{base_ty}`"),
                        base.span,
                    );
                    Type::Error
                }
            }
            ExprKind::Cast { expr: inner, ty } => {
                let source_ty = self.infer(inner);
                let target_ty = self.resolve_type(ty);
                if source_ty == Type::Error || target_ty == Type::Error {
                    Type::Error
                } else if self.can_cast(source_ty, target_ty) {
                    target_ty
                } else {
                    self.error(
                        "NXS022",
                        format!("cannot cast `{source_ty}` to `{target_ty}`"),
                        expr.span,
                    );
                    Type::Error
                }
            }
            ExprKind::Error => Type::Error,
        };

        self.analysis.expr_types.insert(expr.span, ty);
        ty
    }

    fn check_call(&mut self, callee: &str, callee_span: Span, args: &[Expr], span: Span) -> Type {
        if callee == "print" || callee == "println" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!(
                        "`{callee}` expects exactly one argument, found {}",
                        args.len()
                    ),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty != Type::Error && !arg_ty.is_printable() {
                self.error(
                    "NXS015",
                    format!("`{callee}` cannot print a value of type `{arg_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::Void;
        }

        if let Some(info) = self.analysis.structs.get(callee).cloned() {
            if args.len() != info.fields.len() {
                self.error(
                    "NXS004",
                    format!(
                        "struct `{callee}` expects {} field value(s), found {}",
                        info.fields.len(),
                        args.len()
                    ),
                    span,
                );
            }
            for (index, arg) in args.iter().enumerate() {
                if let Some((_, ty)) = info.fields.get(index) {
                    self.check_expected(arg, *ty);
                } else {
                    self.infer(arg);
                }
            }
            return Type::Struct(info.name_static);
        }

        let Some(sig) = self.analysis.functions.get(callee).cloned() else {
            self.error(
                "NXS004",
                format!("unknown function `{callee}`"),
                callee_span,
            );
            for arg in args {
                self.infer(arg);
            }
            return Type::Error;
        };

        if args.len() != sig.params.len() {
            self.error(
                "NXS004",
                format!(
                    "function `{callee}` expects {} argument(s), found {}",
                    sig.params.len(),
                    args.len()
                ),
                span,
            );
        }

        for (index, arg) in args.iter().enumerate() {
            if let Some(expected) = sig.params.get(index) {
                self.check_expected(arg, *expected);
            } else {
                self.infer(arg);
            }
        }

        sig.ret
    }

    fn unify_operands(&mut self, left: &Expr, right: &Expr) -> Option<Type> {
        let left_ty = self.infer(left);
        let right_ty = self.infer(right);
        if left_ty == Type::Error || right_ty == Type::Error {
            return None;
        }
        if left_ty == right_ty {
            return Some(left_ty);
        }
        if self.coerce_literal(right, left_ty) {
            return Some(left_ty);
        }
        if self.coerce_literal(left, right_ty) {
            return Some(right_ty);
        }
        self.error(
            "NXS008",
            format!(
                "operands have different types `{left_ty}` and `{right_ty}`; NEX 0.1 does not apply implicit widening"
            ),
            left.span.merge(right.span),
        );
        None
    }

    fn coerce_literal(&mut self, expr: &Expr, expected: Type) -> bool {
        let fits = match &expr.kind {
            ExprKind::Int(value) => {
                if let Some(range) = expected.integer_range() {
                    let value = *value as i128;
                    value >= range.0 && value <= range.1
                } else {
                    false
                }
            }
            ExprKind::Float(_) => expected == Type::F32 || expected == Type::F64,
            _ => false,
        };
        if fits {
            self.analysis.expr_types.insert(expr.span, expected);
            true
        } else {
            false
        }
    }

    fn can_cast(&self, source: Type, target: Type) -> bool {
        if source == target {
            return true;
        }
        let source_numeric = source.is_numeric() || source == Type::Bool || source == Type::Char;
        let target_numeric = target.is_numeric() || target == Type::Bool || target == Type::Char;
        source_numeric && target_numeric
    }

    fn resolve_type(&mut self, ty: &nex_ast::TypeExpr) -> Type {
        if let Some(resolved) = Type::from_name(&ty.name) {
            if resolved == Type::Void {
                // `void` is only valid as a function return type. Callers can still
                // accept it; parameter/variable checks happen at the call site.
            }
            return resolved;
        }
        if let Some(info) = self.analysis.structs.get(&ty.name) {
            return Type::Struct(info.name_static);
        }
        if let Some(info) = self.analysis.enums.get(&ty.name) {
            return Type::Enum(info.name_static);
        }
        self.error("NXS002", format!("unknown type `{}`", ty.name), ty.span);
        Type::Error
    }

    fn declare(&mut self, name: &str, ty: Type, mutable: bool, span: Span) {
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        if scope.contains_key(name) {
            self.error(
                "NXS013",
                format!("duplicate variable `{name}` in the same scope"),
                span,
            );
            return;
        }
        scope.insert(name.to_string(), VarInfo { ty, mutable });
    }

    fn lookup(&mut self, name: &str) -> Option<VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.get(name) {
                return Some(var.clone());
            }
        }
        None
    }

    fn block_returns(&self, block: &nex_ast::Block) -> bool {
        block
            .stmts
            .iter()
            .any(|statement| self.stmt_returns(statement))
    }

    fn stmt_returns(&self, statement: &Stmt) -> bool {
        match &statement.kind {
            StmtKind::Return(_) => true,
            StmtKind::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.block_returns(&arm.body))
            }
            StmtKind::If {
                branches,
                else_branch: Some(else_block),
            } => {
                !branches.is_empty()
                    && branches.iter().all(|(_, block)| self.block_returns(block))
                    && self.block_returns(else_block)
            }
            _ => false,
        }
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.analysis
            .diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    fn warning(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.analysis
            .diagnostics
            .push(Diagnostic::warning(code, message, span));
    }
}

fn intern_array_type(elem: Type, len: usize) -> (&'static str, &'static str) {
    let display = Box::leak(format!("[{}; {}]", elem, len).into_boxed_str());
    let llvm = Box::leak(format!("[{} x {}]", len, llvm_type_name(elem)).into_boxed_str());
    (display, llvm)
}

fn llvm_type_name(ty: Type) -> String {
    match ty {
        Type::Bool => "i1".to_string(),
        Type::Char => "i32".to_string(),
        Type::I8 | Type::U8 => "i8".to_string(),
        Type::I16 | Type::U16 => "i16".to_string(),
        Type::I32 | Type::U32 => "i32".to_string(),
        Type::I64 | Type::U64 => "i64".to_string(),
        Type::F32 => "float".to_string(),
        Type::F64 => "double".to_string(),
        Type::String => "ptr".to_string(),
        Type::Struct(name) => format!("%struct.{}", name),
        Type::Enum(_) => "i32".to_string(),
        Type::Array { llvm, .. } => llvm.to_string(),
        _ => "void".to_string(),
    }
}
