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
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    /// Interned copy of `name`, used by [`Type::Struct`].
    pub name_static: &'static str,
    pub fields: Vec<(String, Type)>,
    pub public: bool,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub name_static: &'static str,
    pub variants: Vec<String>,
    pub public: bool,
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
    pub modules: std::collections::HashSet<String>,
    pub aliases: std::collections::HashMap<String, String>,
    pub public_consts: std::collections::HashSet<String>,
    pub externs: std::collections::HashSet<String>,
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
    pub field_writes: HashMap<Span, Vec<(&'static str, u32, Type)>>,
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

    pub fn field_write(&self, span: Span) -> Option<Vec<(&'static str, u32, Type)>> {
        self.field_writes.get(&span).cloned()
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
                modules: std::collections::HashSet::new(),
                aliases: std::collections::HashMap::new(),
                public_consts: std::collections::HashSet::new(),
                externs: std::collections::HashSet::new(),
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
        checker.collect_modules();
        checker.collect_structs();
        checker.collect_enums();
        checker.collect_consts();
        checker.collect_functions();
        checker.check_uses();
        checker.check_main();
        checker.check_functions();
        checker.analysis
    }

    fn collect_functions(&mut self) {
        for function in &self.program.functions {
            if function.extern_abi.is_some() {
                self.analysis.externs.insert(function.name.clone());
            }

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
                params.push(ty);
            }

            let ret = function
                .ret
                .as_ref()
                .map(|ty| self.resolve_type(ty))
                .unwrap_or(Type::Void);

            self.analysis.functions.insert(
                function.name.clone(),
                FnSig {
                    name: function.name.clone(),
                    params,
                    ret,
                    public: function.public,
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
                    public: definition.public,
                },
            );
        }
    }

    fn collect_modules(&mut self) {
        for name in &self.program.modules {
            self.analysis.modules.insert(name.clone());
        }
        for use_decl in &self.program.uses {
            if use_decl.path.len() == 1 {
                // `use foo` makes `foo` available (already a module name).
                continue;
            }
            if use_decl.path.len() >= 2 {
                let qualified = use_decl.path.join(".");
                let short = use_decl.path.last().cloned().unwrap();
                self.analysis.aliases.insert(short, qualified);
            }
        }
    }

    fn check_uses(&mut self) {
        for use_decl in &self.program.uses.clone() {
            if use_decl.path.len() < 2 {
                continue;
            }
            let qualified = use_decl.path.join(".");
            // Validate existence and visibility.
            let mut found = false;
            let mut public = true;
            if let Some(ty) = self.analysis.consts.get(&qualified).copied() {
                let _ = ty;
                found = true;
                public = self.analysis.public_consts.contains(&qualified);
            } else if let Some(info) = self.analysis.structs.get(&qualified) {
                found = true;
                public = info.public;
            } else if let Some(info) = self.analysis.enums.get(&qualified) {
                found = true;
                public = info.public;
            } else if let Some(info) = self.analysis.functions.get(&qualified) {
                found = true;
                public = info.public;
            }
            if !found {
                self.error(
                    "NXS001",
                    format!("unknown item `{}`", qualified),
                    use_decl.span,
                );
                continue;
            }
            if !public {
                self.error(
                    "NXS038",
                    format!("item `{}` is private", qualified),
                    use_decl.span,
                );
            }
        }
    }

    fn collect_structs(&mut self) {
        // First register every struct name so that field types can refer to
        // structs declared later in the same program (including nested fields).
        for definition in &self.program.structs {
            if self.analysis.structs.contains_key(&definition.name) {
                self.error(
                    "NXS003",
                    format!("duplicate struct `{}`", definition.name),
                    definition.span,
                );
                continue;
            }
            let name_static: &'static str = Box::leak(definition.name.clone().into_boxed_str());
            self.analysis.structs.insert(
                definition.name.clone(),
                StructInfo {
                    name: definition.name.clone(),
                    name_static,
                    fields: Vec::new(),
                    public: definition.public,
                },
            );
        }

        for definition in &self.program.structs {
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
                if let Type::Struct(nested) = ty
                    && nested == definition.name
                {
                    self.error(
                        "NXS025",
                        format!(
                            "field `{}` has recursive struct type `{ty}`; recursive struct fields are not supported",
                            field.name
                        ),
                        field.ty.span,
                    );
                    continue;
                }
                fields.push((field.name.clone(), ty));
            }

            if let Some(info) = self.analysis.structs.get_mut(&definition.name) {
                info.fields = fields;
            }
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

            self.analysis
                .consts
                .insert(constant.name.clone(), declared_ty);
            self.analysis
                .const_exprs
                .insert(constant.name.clone(), constant.init.clone());
            if constant.public {
                self.analysis.public_consts.insert(constant.name.clone());
            }
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
            ExprKind::Array(elements) => elements
                .iter()
                .all(|element| self.is_const_expr(element, visiting)),
            ExprKind::Tuple(elements) => elements
                .iter()
                .all(|element| self.is_const_expr(element, visiting)),
            ExprKind::Call { callee, args, .. } => {
                let resolved = self
                    .analysis
                    .aliases
                    .get(callee)
                    .cloned()
                    .unwrap_or_else(|| callee.clone());
                self.analysis.structs.contains_key(&resolved)
                    && args.iter().all(|arg| self.is_const_expr(arg, visiting))
            }
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
            if self.analysis.externs.contains(&function.name) {
                continue;
            }
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
            StmtKind::AssignField { base, path, value } => {
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
                let mut current_ty = var.ty;
                let mut steps = Vec::with_capacity(path.len());
                let mut failed = false;
                for field in path {
                    if let Type::Struct(struct_name) = current_ty {
                        let info = self.analysis.structs.get(struct_name).cloned();
                        if let Some(info) = info {
                            if let Some(index) =
                                info.fields.iter().position(|(name, _)| name == field)
                            {
                                let field_ty = info.fields[index].1;
                                steps.push((info.name_static, index as u32, field_ty));
                                current_ty = field_ty;
                            } else {
                                self.error(
                                    "NXS027",
                                    format!("struct `{struct_name}` has no field `{field}`"),
                                    statement.span,
                                );
                                failed = true;
                                break;
                            }
                        } else {
                            failed = true;
                            break;
                        }
                    } else {
                        self.error(
                            "NXS027",
                            format!(
                                "cannot assign field `{field}` on a value of type `{current_ty}`"
                            ),
                            statement.span,
                        );
                        failed = true;
                        break;
                    }
                }
                if failed {
                    self.infer(value);
                    return;
                }
                if current_ty == Type::Void {
                    self.error(
                        "NXS002",
                        "field cannot have type `void`".to_string(),
                        statement.span,
                    );
                    self.infer(value);
                    return;
                }
                self.analysis.field_writes.insert(statement.span, steps);
                self.check_expected(value, current_ty);
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
            StmtKind::Pass => {}
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
                if let Some(qualified) = self.analysis.aliases.get(name).cloned() {
                    // Resolve the alias by walking the qualified path.
                    let mut current = Expr {
                        kind: ExprKind::Ident(qualified.split('.').next().unwrap().to_string()),
                        span: expr.span,
                    };
                    for part in qualified.split('.').skip(1) {
                        current = Expr {
                            kind: ExprKind::Field {
                                base: Box::new(current),
                                name: part.to_string(),
                                name_span: expr.span,
                            },
                            span: expr.span,
                        };
                    }
                    let ty = self.infer(&current);
                    self.analysis.expr_types.insert(expr.span, ty);
                    return ty;
                }
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
                            || matches!(ty, Type::Struct(_) | Type::Array { .. })
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
                    && self.analysis.modules.contains(base_name)
                {
                    let qualified = format!("{}.{}", base_name, name);
                    if let Some(ty) = self.analysis.consts.get(&qualified).copied() {
                        if !self.analysis.public_consts.contains(&qualified) {
                            self.error(
                                "NXS038",
                                format!("item `{name}` in module `{base_name}` is private"),
                                *name_span,
                            );
                            self.analysis.expr_types.insert(expr.span, Type::Error);
                            return Type::Error;
                        }
                        self.analysis.expr_types.insert(expr.span, ty);
                        return ty;
                    }
                    if let Some(info) = self.analysis.structs.get(&qualified).cloned() {
                        if !info.public {
                            self.error(
                                "NXS038",
                                format!("item `{name}` in module `{base_name}` is private"),
                                *name_span,
                            );
                            self.analysis.expr_types.insert(expr.span, Type::Error);
                            return Type::Error;
                        }
                        let ty = Type::Struct(info.name_static);
                        self.analysis.expr_types.insert(expr.span, ty);
                        return ty;
                    }
                    if let Some(info) = self.analysis.enums.get(&qualified).cloned() {
                        if !info.public {
                            self.error(
                                "NXS038",
                                format!("item `{name}` in module `{base_name}` is private"),
                                *name_span,
                            );
                            self.analysis.expr_types.insert(expr.span, Type::Error);
                            return Type::Error;
                        }
                        let ty = Type::Enum(info.name_static);
                        self.analysis.expr_types.insert(expr.span, ty);
                        return ty;
                    }
                    self.error(
                        "NXS001",
                        format!("unknown item `{name}` in module `{base_name}`"),
                        *name_span,
                    );
                    self.analysis.expr_types.insert(expr.span, Type::Error);
                    return Type::Error;
                }

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
                    } else if let Type::Enum(enum_name) = base_ty {
                        let info = self.analysis.enums.get(enum_name).cloned();
                        if let Some(info) = info
                            && let Some(index) =
                                info.variants.iter().position(|variant| variant == name)
                        {
                            self.analysis
                                .variant_values
                                .insert(expr.span, (info.name_static, index as u32));
                            Type::Enum(info.name_static)
                        } else {
                            self.error(
                                "NXS030",
                                format!("enum `{enum_name}` has no variant `{name}`"),
                                *name_span,
                            );
                            Type::Error
                        }
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
            ExprKind::Tuple(elements) => {
                if elements.is_empty() {
                    self.error(
                        "NXS037",
                        "empty tuples are not supported in NEX 0.2".to_string(),
                        expr.span,
                    );
                    Type::Error
                } else {
                    let mut elem_types = Vec::with_capacity(elements.len());
                    let mut failed = false;
                    for element in elements {
                        let ty = self.infer(element);
                        if ty == Type::Error {
                            failed = true;
                        } else {
                            elem_types.push(ty);
                        }
                    }
                    if failed || elem_types.len() != elements.len() {
                        Type::Error
                    } else {
                        let name = intern_tuple_name(&elem_types);
                        if !self.analysis.structs.contains_key(name) {
                            let fields: Vec<(String, Type)> = elem_types
                                .iter()
                                .enumerate()
                                .map(|(index, ty)| (index.to_string(), *ty))
                                .collect();
                            self.analysis.structs.insert(
                                name.to_string(),
                                StructInfo {
                                    name: name.to_string(),
                                    name_static: name,
                                    fields,
                                    public: false,
                                },
                            );
                        }
                        Type::Struct(name)
                    }
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
        let callee = self
            .analysis
            .aliases
            .get(callee)
            .cloned()
            .unwrap_or_else(|| callee.to_string());
        let callee = callee.as_str();
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

        if callee == "len" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`len` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty == Type::Error {
                return Type::Error;
            }
            if arg_ty == Type::String || matches!(arg_ty, Type::Array { .. }) {
                return Type::I64;
            }
            self.error(
                "NXS004",
                format!("`len` expects a `string` or array, found `{arg_ty}`"),
                args[0].span,
            );
            return Type::Error;
        }

        if callee == "abs" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`abs` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty == Type::I64 {
                return Type::I64;
            }
            if arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`abs` expects `i64`, found `{arg_ty}`"),
                    args[0].span,
                );
            }
            return Type::Error;
        }

        if callee == "min" || callee == "max" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let left_ty = self.infer(&args[0]);
            let right_ty = self.infer(&args[1]);
            if left_ty == Type::Error || right_ty == Type::Error {
                return Type::Error;
            }
            if left_ty != right_ty {
                self.error(
                    "NXS008",
                    format!("operands have different types `{left_ty}` and `{right_ty}`"),
                    args[0].span.merge(args[1].span),
                );
                return Type::Error;
            }
            if left_ty == Type::I64 || left_ty == Type::F64 {
                return left_ty;
            }
            self.error(
                "NXS004",
                format!("`{callee}` expects `i64` or `f64`, found `{left_ty}`"),
                args[0].span,
            );
            return Type::Error;
        }

        if callee == "sqrt" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`sqrt` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty == Type::F64 {
                return Type::F64;
            }
            if arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`sqrt` expects `f64`, found `{arg_ty}`"),
                    args[0].span,
                );
            }
            return Type::Error;
        }

        if callee == "pow" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`pow` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let base_ty = self.infer(&args[0]);
            let exp_ty = self.infer(&args[1]);
            if base_ty != Type::F64 || exp_ty != Type::F64 {
                if base_ty != Type::Error && exp_ty != Type::Error {
                    self.error(
                        "NXS004",
                        format!("`pow` expects `f64` arguments, found `{base_ty}` and `{exp_ty}`"),
                        span,
                    );
                }
                return Type::Error;
            }
            return Type::F64;
        }

        if callee == "assert" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`assert` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty != Type::Bool && arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`assert` expects `bool`, found `{arg_ty}`"),
                    args[0].span,
                );
            }
            return Type::Void;
        }

        if callee == "panic" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`panic` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty != Type::String && arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`panic` expects `string`, found `{arg_ty}`"),
                    args[0].span,
                );
            }
            return Type::Void;
        }

        if callee == "alloc" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`alloc` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty != Type::I64 && arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`alloc` expects `i64`, found `{arg_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "free" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`free` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty != Type::I64 && arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`free` expects `i64`, found `{arg_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::Void;
        }

        if callee == "peek" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`peek` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty != Type::I64 && arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`peek` expects `i64`, found `{arg_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "poke" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`poke` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let ptr_ty = self.infer(&args[0]);
            let value_ty = self.infer(&args[1]);
            if ptr_ty != Type::I64 && ptr_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`poke` expects `i64` pointer, found `{ptr_ty}`"),
                    args[0].span,
                );
            }
            if value_ty != Type::I64 && value_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`poke` expects `i64` value, found `{value_ty}`"),
                    args[1].span,
                );
            }
            return Type::Void;
        }

        if callee == "concat" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`concat` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let left_ty = self.infer(&args[0]);
            let right_ty = self.infer(&args[1]);
            if (left_ty != Type::String || right_ty != Type::String)
                && (left_ty != Type::Error && right_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    format!(
                        "`concat` expects `string` arguments, found `{left_ty}` and `{right_ty}`"
                    ),
                    span,
                );
                return Type::Error;
            }
            if left_ty == Type::Error || right_ty == Type::Error {
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "str_contains" || callee == "str_index_of" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let left_ty = self.infer(&args[0]);
            let right_ty = self.infer(&args[1]);
            if (left_ty != Type::String || right_ty != Type::String)
                && (left_ty != Type::Error && right_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    format!(
                        "`{callee}` expects `string` arguments, found `{left_ty}` and `{right_ty}`"
                    ),
                    span,
                );
                return Type::Error;
            }
            if left_ty == Type::Error || right_ty == Type::Error {
                return Type::Error;
            }
            return if callee == "str_contains" {
                Type::Bool
            } else {
                Type::I64
            };
        }

        if callee == "str_substring" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!("`str_substring` expects 3 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let value_ty = self.infer(&args[0]);
            let start_ty = self.infer(&args[1]);
            let end_ty = self.infer(&args[2]);
            let numeric = start_ty == Type::I64 && end_ty == Type::I64;
            if (value_ty != Type::String || !numeric)
                && (value_ty != Type::Error && start_ty != Type::Error && end_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    "`str_substring` expects `(string, i64, i64)`, found `({value_ty}, {start_ty}, {end_ty})`",
                    span,
                );
                return Type::Error;
            }
            if value_ty == Type::Error || start_ty == Type::Error || end_ty == Type::Error {
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "str_upper" || callee == "str_lower" || callee == "str_trim" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let value_ty = self.infer(&args[0]);
            if value_ty != Type::String && value_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a `string`, found `{value_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return value_ty;
        }

        if callee == "str_starts_with" || callee == "str_ends_with" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let value_ty = self.infer(&args[0]);
            let other_ty = self.infer(&args[1]);
            if (value_ty != Type::String || other_ty != Type::String)
                && (value_ty != Type::Error && other_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    format!(
                        "`{callee}` expects `string` arguments, found `{value_ty}` and `{other_ty}`"
                    ),
                    span,
                );
                return Type::Error;
            }
            if value_ty == Type::Error || other_ty == Type::Error {
                return Type::Error;
            }
            return Type::Bool;
        }

        if callee == "str_repeat" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`str_repeat` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let value_ty = self.infer(&args[0]);
            let count_ty = self.infer(&args[1]);
            if (value_ty != Type::String || count_ty != Type::I64)
                && (value_ty != Type::Error && count_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    format!(
                        "`str_repeat` expects `(string, i64)`, found `({value_ty}, {count_ty})`"
                    ),
                    span,
                );
                return Type::Error;
            }
            if value_ty == Type::Error || count_ty == Type::Error {
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "str_replace" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!("`str_replace` expects 3 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let value_ty = self.infer(&args[0]);
            let from_ty = self.infer(&args[1]);
            let to_ty = self.infer(&args[2]);
            let all_strings =
                value_ty == Type::String && from_ty == Type::String && to_ty == Type::String;
            if !all_strings
                && value_ty != Type::Error
                && from_ty != Type::Error
                && to_ty != Type::Error
            {
                self.error(
                    "NXS004",
                    "`str_replace` expects three `string` arguments",
                    span,
                );
                return Type::Error;
            }
            if value_ty == Type::Error || from_ty == Type::Error || to_ty == Type::Error {
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "to_string" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`to_string` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if arg_ty == Type::I64 {
                return Type::String;
            }
            if arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`to_string` expects `i64`, found `{arg_ty}`"),
                    args[0].span,
                );
            }
            return Type::Error;
        }

        if let Some(info) = self.analysis.structs.get(callee).cloned() {
            if callee.contains('.') && !info.public {
                self.error(
                    "NXS038",
                    format!("item `{}` is private", callee),
                    callee_span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
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

        if callee.contains('.') && !sig.public {
            self.error(
                "NXS038",
                format!("function `{callee}` is private"),
                callee_span,
            );
            for arg in args {
                self.infer(arg);
            }
            return Type::Error;
        }

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
        if let Some(elems) = &ty.tuple {
            if elems.is_empty() {
                self.error(
                    "NXS037",
                    "empty tuples are not supported".to_string(),
                    ty.span,
                );
                return Type::Error;
            }
            let mut elem_types = Vec::with_capacity(elems.len());
            for elem in elems {
                let resolved = self.resolve_type(elem);
                if resolved == Type::Error {
                    return Type::Error;
                }
                elem_types.push(resolved);
            }
            let name = intern_tuple_name(&elem_types);
            if !self.analysis.structs.contains_key(name) {
                let fields: Vec<(String, Type)> = elem_types
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| (index.to_string(), *ty))
                    .collect();
                self.analysis.structs.insert(
                    name.to_string(),
                    StructInfo {
                        name: name.to_string(),
                        name_static: name,
                        fields,
                        public: false,
                    },
                );
            }
            return Type::Struct(name);
        }

        if let Some((elem_ty_expr, len)) = &ty.array {
            let elem_ty = self.resolve_type(elem_ty_expr);
            if elem_ty == Type::Error {
                return Type::Error;
            }
            if *len == 0 {
                self.error(
                    "NXS035",
                    "empty array types are not supported".to_string(),
                    ty.span,
                );
                return Type::Error;
            }
            let (display, llvm) = intern_array_type(elem_ty, *len);
            if !self.analysis.arrays.contains_key(display) {
                self.analysis.arrays.insert(
                    display.to_string(),
                    ArrayInfo {
                        display,
                        llvm,
                        elem: elem_ty,
                        len: *len,
                    },
                );
            }
            return Type::Array { display, llvm };
        }

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
            StmtKind::If {
                branches,
                else_branch: None,
            } => {
                branches.len() == 1
                    && self.const_bool(&branches[0].0).unwrap_or(false)
                    && self.block_returns(&branches[0].1)
            }
            StmtKind::While { cond, body } => {
                self.const_bool(cond) == Some(true) && self.block_returns(body)
            }
            _ => false,
        }
    }

    fn const_bool(&self, expr: &Expr) -> Option<bool> {
        match &expr.kind {
            ExprKind::Bool(value) => Some(*value),
            ExprKind::Unary {
                op: nex_ast::UnaryOp::Not,
                operand,
            } => self.const_bool(operand).map(|value| !value),
            ExprKind::Binary { op, left, right } => match op {
                nex_ast::BinOp::And => Some(self.const_bool(left)? && self.const_bool(right)?),
                nex_ast::BinOp::Or => Some(self.const_bool(left)? || self.const_bool(right)?),
                nex_ast::BinOp::Eq => Some(self.const_bool(left)? == self.const_bool(right)?),
                nex_ast::BinOp::Ne => Some(self.const_bool(left)? != self.const_bool(right)?),
                _ => None,
            },
            _ => None,
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

fn intern_tuple_name(elem_types: &[Type]) -> &'static str {
    let signature: String = elem_types
        .iter()
        .map(|ty| sanitize_type_name(ty.name()))
        .collect::<Vec<_>>()
        .join("_");
    Box::leak(format!("tuple_{}_{}", elem_types.len(), signature).into_boxed_str())
}

fn sanitize_type_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
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
