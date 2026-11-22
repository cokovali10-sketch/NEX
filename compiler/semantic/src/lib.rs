mod moves;
mod types;

pub use types::Type;

use std::sync::atomic::{AtomicUsize, Ordering};

// Mono instantiations get a distinct span region so the span-keyed semantic
// maps do not collide between copies of the same generic function.
const MONO_SPAN_BASE: usize = 2_000_000_000;
const MONO_SPAN_STRIDE: usize = 10_000;

static MONO_SPAN_COUNTER: AtomicUsize = AtomicUsize::new(0);

use nex_ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use nex_diagnostics::{Diagnostic, Span};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone)]
pub struct FnSig {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Type>,
    pub ret: Type,
    pub public: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub type_params: Vec<String>,
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
pub struct TraitMethodSig {
    pub name: String,
    pub params: Vec<Type>,
    pub ret: Type,
}

#[derive(Debug, Clone)]
pub struct TraitInfo {
    pub name: String,
    pub methods: Vec<TraitMethodSig>,
    pub public: bool,
}

/// Resolution of `receiver.method(...)` stored by span for the IR builder.
#[derive(Debug, Clone)]
pub struct MethodCallInfo {
    pub symbol: String,
    pub receiver: String,
    pub receiver_ty: Type,
    pub ret: Type,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub structs: BTreeMap<String, StructInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    pub arrays: BTreeMap<String, ArrayInfo>,
    pub traits: BTreeMap<String, TraitInfo>,
    pub type_aliases: BTreeMap<String, Type>,
    pub modules: std::collections::HashSet<String>,
    pub aliases: std::collections::HashMap<String, String>,
    pub public_consts: std::collections::HashSet<String>,
    pub externs: std::collections::HashSet<String>,
    /// Variant name to the interned name of the enum that declares it.
    pub variant_map: HashMap<String, &'static str>,
    pub consts: BTreeMap<String, Type>,
    /// Module-level `var` bindings, available to every function and thread.
    pub globals: BTreeMap<String, GlobalInfo>,
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
    /// Method calls keyed by the full call expression span.
    pub method_calls: HashMap<Span, MethodCallInfo>,
    /// Monomorphized generic functions produced during semantic analysis.
    /// Monomorphized generic functions produced during semantic analysis.
    pub mono_functions: Vec<nex_ast::FnDecl>,
    /// Names of generic (un-monomorphized) functions that must not be compiled.
    pub generic_functions: Vec<String>,
    pub mono_structs: HashMap<Span, String>,
    pub mono_calls: HashMap<Span, String>,
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

    pub fn method_call(&self, span: Span) -> Option<&MethodCallInfo> {
        self.method_calls.get(&span)
    }

    pub fn mono_struct(&self, span: Span) -> Option<&String> {
        self.mono_structs.get(&span)
    }

    pub fn mono_call(&self, span: Span) -> Option<&String> {
        self.mono_calls.get(&span)
    }

    pub fn expr_type(&self, span: Span) -> Option<Type> {
        self.expr_types.get(&span).copied()
    }

    pub fn decl_type(&self, span: Span) -> Option<Type> {
        self.decl_types.get(&span).copied()
    }
}

pub use types::Type as NexType;

#[derive(Debug, Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
}

/// A module-level `var`: its type plus the runtime slot the compiler assigns.
#[derive(Debug, Clone)]
pub struct GlobalInfo {
    pub ty: Type,
    pub mutable: bool,
    pub slot: i64,
}

struct Checker<'a> {
    program: &'a Program,
    analysis: Analysis,
    scopes: Vec<HashMap<String, VarInfo>>,
    current_fn: Option<FnSig>,
    current_type_params: Vec<String>,
    loop_depth: usize,
}

fn substitute_type_expr(
    ty: &nex_ast::TypeExpr,
    mapping: &std::collections::HashMap<String, Type>,
) -> nex_ast::TypeExpr {
    let mut result = ty.clone();
    if let Some(concrete) = mapping.get(&ty.name) {
        result.name = concrete.name().to_string();
        result.generic_args.clear();
    } else {
        result.generic_args = ty
            .generic_args
            .iter()
            .map(|arg| substitute_type_expr(arg, mapping))
            .collect();
    }
    if let Some((elem, size)) = &ty.array {
        result.array = Some((Box::new(substitute_type_expr(elem, mapping)), *size));
    }
    if let Some(elems) = &ty.tuple {
        result.tuple = Some(
            elems
                .iter()
                .map(|e| substitute_type_expr(e, mapping))
                .collect(),
        );
    }
    result
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
                traits: BTreeMap::new(),
                type_aliases: BTreeMap::new(),
                modules: std::collections::HashSet::new(),
                aliases: std::collections::HashMap::new(),
                public_consts: std::collections::HashSet::new(),
                externs: std::collections::HashSet::new(),
                variant_map: HashMap::new(),
                consts: BTreeMap::new(),
                globals: BTreeMap::new(),
                const_exprs: BTreeMap::new(),
                functions: BTreeMap::new(),
                expr_types: HashMap::new(),
                decl_types: HashMap::new(),
                field_reads: HashMap::new(),
                field_writes: HashMap::new(),
                variant_values: HashMap::new(),
                method_calls: HashMap::new(),
                mono_functions: Vec::new(),
                generic_functions: Vec::new(),
                mono_structs: HashMap::new(),
                mono_calls: HashMap::new(),
                diagnostics: Vec::new(),
            },
            scopes: Vec::new(),
            current_fn: None,
            current_type_params: Vec::new(),
            loop_depth: 0,
        };
        checker.collect_prelude();
        checker.collect_modules();
        checker.collect_structs();
        checker.collect_enums();
        checker.collect_traits();
        checker.collect_impls();
        checker.collect_type_aliases();
        checker.collect_consts();
        checker.collect_globals();
        checker.collect_functions();
        checker.check_uses();
        checker.check_main();
        checker.check_functions();
        checker
            .analysis
            .diagnostics
            .extend(moves::check_moves(program, &checker.analysis));
        checker.analysis
    }

    /// Registers module-level `var` bindings and type-checks their initialisers.
    fn collect_globals(&mut self) {
        for (slot, global) in self.program.globals.iter().enumerate() {
            let declared = global
                .ty
                .as_ref()
                .map(|annotation| self.resolve_type(annotation));
            let inferred = self.infer(&global.init);
            let ty = match declared {
                Some(expected) => {
                    if expected == Type::Error || inferred == Type::Error {
                        Type::Error
                    } else {
                        self.check_expected(&global.init, expected);
                        expected
                    }
                }
                None => inferred,
            };
            if !global.mutable {
                self.error(
                    "NXS007",
                    format!("module-level `{}` must be declared with `var`", global.name),
                    global.span,
                );
            }
            if ty != Type::Error {
                self.analysis.globals.insert(
                    global.name.clone(),
                    GlobalInfo {
                        ty,
                        mutable: true,
                        slot: slot as i64,
                    },
                );
            }
        }
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

            self.current_type_params = function.type_params.clone();
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

            self.current_type_params.clear();

            self.analysis.functions.insert(
                function.name.clone(),
                FnSig {
                    name: function.name.clone(),
                    type_params: function.type_params.clone(),
                    params,
                    ret,
                    public: function.public,
                    span: function.span,
                },
            );
            if !function.type_params.is_empty() {
                self.analysis.generic_functions.push(function.name.clone());
            }
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

    fn collect_traits(&mut self) {
        for definition in &self.program.traits {
            if self.analysis.structs.contains_key(&definition.name)
                || self.analysis.enums.contains_key(&definition.name)
                || self.analysis.functions.contains_key(&definition.name)
            {
                self.error(
                    "NXS003",
                    format!("name `{}` is already declared", definition.name),
                    definition.span,
                );
                continue;
            }
            if self.analysis.traits.contains_key(&definition.name) {
                self.error(
                    "NXS039",
                    format!("duplicate trait `{}`", definition.name),
                    definition.span,
                );
                continue;
            }

            let mut methods = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for method in &definition.methods {
                if !seen.insert(method.name.clone()) {
                    self.error(
                        "NXS003",
                        format!(
                            "duplicate method `{}` in trait `{}`",
                            method.name, definition.name
                        ),
                        method.span,
                    );
                    continue;
                }
                let mut params = Vec::with_capacity(method.params.len());
                for param in &method.params {
                    let ty = self.resolve_type(&param.ty);
                    if ty == Type::Void {
                        self.error(
                            "NXS002",
                            format!("parameter `{}` cannot have type `void`", param.name),
                            param.span,
                        );
                    }
                    params.push(ty);
                }
                let ret = method
                    .ret
                    .as_ref()
                    .map(|ty| self.resolve_type(ty))
                    .unwrap_or(Type::Void);
                methods.push(TraitMethodSig {
                    name: method.name.clone(),
                    params,
                    ret,
                });
            }

            self.analysis.traits.insert(
                definition.name.clone(),
                TraitInfo {
                    name: definition.name.clone(),
                    methods,
                    public: definition.public,
                },
            );
        }
    }

    fn collect_impls(&mut self) {
        let mut implemented: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for block in &self.program.impls {
            let trait_info = if block.trait_name.is_empty() {
                let methods: Vec<TraitMethodSig> = block
                    .methods
                    .iter()
                    .map(|method| {
                        let params = method
                            .params
                            .iter()
                            .skip(1)
                            .map(|param| self.resolve_type(&param.ty))
                            .collect();
                        let ret = method
                            .ret
                            .as_ref()
                            .map(|ty| self.resolve_type(ty))
                            .unwrap_or(Type::Void);
                        TraitMethodSig {
                            name: method.name.clone(),
                            params,
                            ret,
                        }
                    })
                    .collect();
                TraitInfo {
                    name: String::new(),
                    methods,
                    public: false,
                }
            } else {
                let Some(info) = self.analysis.traits.get(&block.trait_name).cloned() else {
                    self.error(
                        "NXS041",
                        format!("unknown trait `{}` in impl block", block.trait_name),
                        block.span,
                    );
                    continue;
                };
                info
            };
            let Some(struct_info) = self.analysis.structs.get(&block.type_name).cloned() else {
                self.error(
                    "NXS042",
                    format!(
                        "impl target `{}` must be a declared struct",
                        block.type_name
                    ),
                    block.span,
                );
                continue;
            };
            if !implemented.insert((block.trait_name.clone(), block.type_name.clone())) {
                self.error(
                    "NXS043",
                    format!(
                        "duplicate `impl {} for {}` block",
                        block.trait_name, block.type_name
                    ),
                    block.span,
                );
                continue;
            }

            let mut provided = std::collections::HashSet::new();
            for method in &block.methods {
                if !provided.insert(method.name.clone()) {
                    self.error(
                        "NXS003",
                        format!(
                            "duplicate method `{}` in `impl {} for {}`",
                            method.name, block.trait_name, block.type_name
                        ),
                        method.span,
                    );
                    continue;
                }

                let Some(expected) = trait_info
                    .methods
                    .iter()
                    .find(|candidate| candidate.name == method.name)
                    .cloned()
                else {
                    self.error(
                        "NXS044",
                        format!(
                            "method `{}` is not declared by trait `{}`",
                            method.name, block.trait_name
                        ),
                        method.span,
                    );
                    continue;
                };

                let mut params = Vec::with_capacity(method.params.len());
                for param in &method.params {
                    params.push(self.resolve_type(&param.ty));
                }
                let ret = method
                    .ret
                    .as_ref()
                    .map(|ty| self.resolve_type(ty))
                    .unwrap_or(Type::Void);

                let receiver_ok = params.first() == Some(&Type::Struct(struct_info.name_static));
                if !receiver_ok {
                    self.error(
                        "NXS044",
                        format!(
                            "method `{}` must take `self` of type `{}`",
                            method.name, block.type_name
                        ),
                        method.span,
                    );
                    continue;
                }
                let actual_params = &params[1..];
                if actual_params != expected.params.as_slice() || ret != expected.ret {
                    let diagnostic = Diagnostic::error(
                        "NXS044",
                        format!(
                            "method `{}` does not match the signature declared by trait `{}`",
                            method.name, block.trait_name
                        ),
                        method.span,
                    )
                    .with_note(format!(
                        "expected `({})` returning `{}`",
                        expected
                            .params
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                        expected.ret
                    ));
                    self.analysis.diagnostics.push(diagnostic);
                    continue;
                }

                let symbol = format!("{}.{}", block.type_name, method.name);
                if self.analysis.functions.contains_key(&symbol) {
                    self.error(
                        "NXS003",
                        format!("duplicate method `{symbol}`"),
                        method.span,
                    );
                    continue;
                }
                self.analysis.functions.insert(
                    symbol.clone(),
                    FnSig {
                        name: symbol,
                        type_params: Vec::new(),
                        params,
                        ret,
                        public: struct_info.public,
                        span: method.span,
                    },
                );
            }

            for expected in &trait_info.methods {
                if !provided.contains(&expected.name) {
                    self.error(
                        "NXS045",
                        format!(
                            "`impl {} for {}` is missing method `{}`",
                            block.trait_name, block.type_name, expected.name
                        ),
                        block.span,
                    );
                }
            }
        }
    }

    /// Register the built-in generic `Option<T>` and `Result<T, E>` structs.
    ///
    /// They are real generic structs with a `has`/`ok` discriminator, so
    /// they flow through the same monomorphization path as user-declared
    /// structs and compose with the `?` operator without any special
    /// cases. A user-declared `Option`/`Result` is reported as a duplicate
    /// rather than shadowed.
    fn collect_prelude(&mut self) {
        self.collect_prelude_struct("Option", &["T"], &[("has", "bool"), ("value", "T")]);
        self.collect_prelude_struct(
            "Result",
            &["T", "E"],
            &[("ok", "bool"), ("value", "T"), ("error", "E")],
        );
    }

    fn collect_prelude_struct(&mut self, name: &str, params: &[&str], fields: &[(&str, &str)]) {
        if self.analysis.structs.contains_key(name) {
            // The user already declared one; the duplicate is reported by
            // `collect_structs`.
            return;
        }
        self.current_type_params = params.iter().map(ToString::to_string).collect();
        let mut resolved = Vec::with_capacity(fields.len());
        for (field_name, ty_name) in fields {
            let ty = self.resolve_type(&nex_ast::TypeExpr {
                name: ty_name.to_string(),
                array: None,
                tuple: None,
                generic_args: Vec::new(),
                span: Span::empty(0),
            });
            if ty == Type::Error {
                self.current_type_params.clear();
                return;
            }
            resolved.push((field_name.to_string(), ty));
        }
        self.current_type_params.clear();
        let name_static: &'static str = Box::leak(name.to_string().into_boxed_str());
        self.analysis.structs.insert(
            name.to_string(),
            StructInfo {
                name: name.to_string(),
                type_params: params.iter().map(ToString::to_string).collect(),
                name_static,
                fields: resolved,
                public: true,
            },
        );
    }

    /// Monomorphize a generic struct: instantiate `name` for the concrete
    /// type arguments in `mapping`, register the specialized struct in the
    /// analysis table and return its name.
    fn instantiate_generic_struct(
        &mut self,
        template_name: &str,
        concrete_names: &[String],
        mapping: &std::collections::HashMap<String, Type>,
    ) {
        let mono_name = format!("{}_{}", template_name, concrete_names.join("_"));
        if self.analysis.structs.contains_key(&mono_name) {
            return;
        }
        let base_info = self.analysis.structs.get(template_name).cloned();
        if let Some(mut generic_decl) = self
            .program
            .structs
            .iter()
            .find(|decl| decl.name == template_name)
            .cloned()
        {
            generic_decl.name = mono_name.clone();
            for field in &mut generic_decl.fields {
                field.ty = substitute_type_expr(&field.ty, mapping);
            }
            let mut fields = Vec::new();
            for field in &generic_decl.fields {
                let field_ty = self.resolve_type(&field.ty);
                fields.push((field.name.clone(), field_ty));
            }
            let name_static = Box::leak(mono_name.clone().into_boxed_str());
            self.analysis.structs.insert(
                mono_name.clone(),
                StructInfo {
                    name: mono_name,
                    type_params: Vec::new(),
                    name_static,
                    fields,
                    public: base_info.map(|info| info.public).unwrap_or(false),
                },
            );
            return;
        }
        // The struct is not declared in the program (built-in prelude
        // `Option`/`Result`): synthesize the layout from the generic
        // StructInfo.
        if let Some(info) = base_info {
            let fields = info
                .fields
                .iter()
                .map(|(field_name, ty)| {
                    let substituted = match ty {
                        Type::TypeParam(param) => {
                            mapping.get(*param).copied().unwrap_or(Type::Error)
                        }
                        other => *other,
                    };
                    (field_name.clone(), substituted)
                })
                .collect();
            let name_static = Box::leak(mono_name.clone().into_boxed_str());
            self.analysis.structs.insert(
                mono_name.clone(),
                StructInfo {
                    name: mono_name,
                    type_params: Vec::new(),
                    name_static,
                    fields,
                    public: info.public,
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
                    type_params: definition.type_params.clone(),
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

            self.current_type_params = definition.type_params.clone();
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

            self.current_type_params.clear();

            if let Some(info) = self.analysis.structs.get_mut(&definition.name) {
                info.fields = fields;
            }
        }
    }

    fn collect_type_aliases(&mut self) {
        let aliases = self.program.type_aliases.clone();
        for alias in aliases {
            if self.analysis.type_aliases.contains_key(&alias.name) {
                self.error(
                    "NXS003",
                    format!("duplicate type alias `{}`", alias.name),
                    alias.span,
                );
                continue;
            }
            self.current_type_params.clear();
            let ty = self.resolve_type(&alias.ty);
            if ty == Type::Error {
                continue;
            }
            self.analysis.type_aliases.insert(alias.name.clone(), ty);
        }
        self.current_type_params.clear();
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
            self.error("NXS011", "NEX `main` must not accept parameters", sig.span);
        }

        if !matches!(sig.ret, Type::Void | Type::I32) {
            self.error(
                "NXS011",
                format!(
                    "NEX `main` must return `void` or `i32`, found `{}`",
                    sig.ret
                ),
                sig.span,
            );
        }
    }

    fn check_one_function(&mut self, function: &nex_ast::FnDecl) {
        if self.analysis.externs.contains(&function.name) {
            return;
        }
        let Some(sig) = self.analysis.functions.get(&function.name).cloned() else {
            return;
        };
        if !sig.type_params.is_empty() {
            return;
        }
        // Monomorphized instantiations are checked inline at the call site;
        // save and restore the enclosing function's scope instead of clearing it.
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        self.current_fn = Some(sig.clone());
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
        self.scopes = saved_scopes;
        self.loop_depth = saved_loop_depth;
    }

    fn check_functions(&mut self) {
        let mut functions = self.program.functions.clone();
        for block in &self.program.impls {
            for method in &block.methods {
                let mut method = method.clone();
                method.name = format!("{}.{}", block.type_name, method.name);
                functions.push(method);
            }
        }
        for function in functions {
            self.check_one_function(&function);
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
                names,
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

                if let Some(names) = names {
                    let Type::Struct(struct_name) = inferred else {
                        if inferred != Type::Error {
                            self.error(
                                "NXS005",
                                "tuple destructuring requires a tuple or struct initializer"
                                    .to_string(),
                                statement.span,
                            );
                        }
                        for binding in names {
                            self.declare(binding, Type::Error, *mutable, statement.span);
                        }
                        return;
                    };
                    let fields = self
                        .analysis
                        .structs
                        .get(struct_name)
                        .map(|info| info.fields.clone())
                        .unwrap_or_default();
                    if fields.len() != names.len() {
                        self.error(
                            "NXS005",
                            format!(
                                "tuple pattern binds {} name(s), but value has {} field(s)",
                                names.len(),
                                fields.len()
                            ),
                            statement.span,
                        );
                    }
                    for (index, binding) in names.iter().enumerate() {
                        let field_ty = fields.get(index).map(|(_, ty)| *ty).unwrap_or(Type::Error);
                        self.declare(binding, field_ty, *mutable, statement.span);
                    }
                    self.analysis.decl_types.insert(statement.span, inferred);
                    return;
                }

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
            StmtKind::AssignMany { names, values } => {
                if names.len() != values.len() {
                    self.error(
                        "NXS005",
                        format!(
                            "parallel assignment has {} targets and {} values",
                            names.len(),
                            values.len()
                        ),
                        statement.span,
                    );
                }
                for (name, value) in names.iter().zip(values.iter()) {
                    let Some(var) = self.lookup(name) else {
                        self.error(
                            "NXS016",
                            format!("cannot assign to undeclared variable `{name}`"),
                            statement.span,
                        );
                        self.infer(value);
                        continue;
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
                    self.check_expected(value, var.ty);
                }
                for value in values.iter().skip(names.len()) {
                    self.infer(value);
                }
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
            StmtKind::AssignFieldDeref {
                reference,
                path,
                value,
            } => {
                let ref_ty = self.infer(reference);
                let mut current_ty = self.pointee_type(ref_ty);
                if !matches!(ref_ty, Type::Ref { .. }) {
                    self.error(
                        "NXS051",
                        "cannot assign a field through a non-reference value".to_string(),
                        statement.span,
                    );
                    self.infer(value);
                    return;
                }
                let mut failed = false;
                let mut steps = Vec::with_capacity(path.len());
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
                let array_display = match var_ty {
                    Type::Array { display, .. } => Some(display),
                    Type::DynArray(display) => Some(display),
                    _ => None,
                };
                if let Some(display) = array_display {
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
            StmtKind::AssignIndexDeref {
                reference,
                index,
                value,
            } => {
                let ref_ty = self.infer(reference);
                let pointee = self.pointee_type(ref_ty);
                let array_display = match pointee {
                    Type::Array { display, .. } => Some(display),
                    Type::DynArray(display) => Some(display),
                    _ => None,
                };
                if !matches!(ref_ty, Type::Ref { .. }) {
                    self.error(
                        "NXS051",
                        "cannot index-assign through a non-reference value".to_string(),
                        statement.span,
                    );
                    self.infer(index);
                    self.infer(value);
                    return;
                }
                if let Some(display) = array_display {
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
                        format!("cannot index-assign through a reference to `{pointee}`"),
                        statement.span,
                    );
                    self.infer(index);
                    self.infer(value);
                }
            }
            StmtKind::AssignDeref { place, value } => {
                let place_ty = self.infer(place);
                if let Type::Ref { .. } = place_ty {
                    let value_ty = self.infer(value);
                    if value_ty != Type::Error {
                        self.check_expected(value, self.pointee_type(place_ty));
                    }
                } else {
                    self.error(
                        "NXS051",
                        "cannot dereference-assign to a non-reference value".to_string(),
                        statement.span,
                    );
                    self.infer(value);
                }
            }
            StmtKind::Match { scrutinee, arms } => {
                let scrut_ty = self.infer(scrutinee);
                let is_enum = matches!(scrut_ty, Type::Enum(_));
                let is_int = scrut_ty.is_integer();
                let is_bool = scrut_ty == Type::Bool;
                let is_char = scrut_ty == Type::Char;
                let is_str = scrut_ty == Type::String;

                if !is_enum && !is_int && !is_bool && !is_char && !is_str && scrut_ty != Type::Error
                {
                    self.error(
                        "NXS031",
                        format!(
                            "match scrutinee must have an enum, integer, bool, char, or string type, found `{scrut_ty}`"
                        ),
                        statement.span,
                    );
                }

                let mut seen_variants: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut seen_ints: std::collections::HashSet<i64> =
                    std::collections::HashSet::new();
                let mut seen_strs: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut seen_chars: std::collections::HashSet<char> =
                    std::collections::HashSet::new();
                let mut seen_bools: std::collections::HashSet<bool> =
                    std::collections::HashSet::new();
                let mut else_seen = false;
                let mut else_not_last = false;

                for (index, arm) in arms.iter().enumerate() {
                    match &arm.pattern {
                        nex_ast::Pattern::Else => {
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
                        nex_ast::Pattern::Variant(variant) => {
                            if else_seen {
                                self.error(
                                    "NXS034",
                                    "case arm cannot follow an `else` arm".to_string(),
                                    arm.span,
                                );
                            }
                            if !seen_variants.insert(variant.clone()) {
                                self.error(
                                    "NXS032",
                                    format!("duplicate match arm for variant `{variant}`"),
                                    arm.span,
                                );
                            }
                            if is_enum {
                                if let Type::Enum(name) = scrut_ty {
                                    let info = self.analysis.enums.get(name).cloned();
                                    if let Some(info) = info
                                        && !info.variants.iter().any(|value| value == variant)
                                    {
                                        self.error(
                                            "NXS030",
                                            format!("enum `{name}` has no variant `{variant}`"),
                                            arm.span,
                                        );
                                    }
                                }
                            } else if scrut_ty != Type::Error {
                                self.error(
                                    "NXS030",
                                    "variant patterns are only valid for enum scrutinees"
                                        .to_string(),
                                    arm.span,
                                );
                            }
                        }
                        nex_ast::Pattern::Int(value) => {
                            if else_seen {
                                self.error(
                                    "NXS034",
                                    "case arm cannot follow an `else` arm".to_string(),
                                    arm.span,
                                );
                            }
                            if !is_int {
                                if scrut_ty != Type::Error {
                                    self.error(
                                        "NXS030",
                                        "integer patterns require an integer scrutinee".to_string(),
                                        arm.span,
                                    );
                                }
                            } else if !seen_ints.insert(*value) {
                                self.error(
                                    "NXS032",
                                    format!("duplicate match arm for `{value}`"),
                                    arm.span,
                                );
                            }
                        }
                        nex_ast::Pattern::Range(start, end) => {
                            if else_seen {
                                self.error(
                                    "NXS034",
                                    "case arm cannot follow an `else` arm".to_string(),
                                    arm.span,
                                );
                            }
                            if !is_int {
                                if scrut_ty != Type::Error {
                                    self.error(
                                        "NXS030",
                                        "range patterns require an integer scrutinee".to_string(),
                                        arm.span,
                                    );
                                }
                            } else if start > end {
                                self.error(
                                    "NXS030",
                                    format!("invalid range `{start}..{end}`"),
                                    arm.span,
                                );
                            }
                        }
                        nex_ast::Pattern::Bool(value) => {
                            if else_seen {
                                self.error(
                                    "NXS034",
                                    "case arm cannot follow an `else` arm".to_string(),
                                    arm.span,
                                );
                            }
                            if !is_bool {
                                if scrut_ty != Type::Error {
                                    self.error(
                                        "NXS030",
                                        "boolean patterns require a `bool` scrutinee".to_string(),
                                        arm.span,
                                    );
                                }
                            } else if !seen_bools.insert(*value) {
                                self.error(
                                    "NXS032",
                                    format!("duplicate match arm for `{value}`"),
                                    arm.span,
                                );
                            }
                        }
                        nex_ast::Pattern::Char(value) => {
                            if else_seen {
                                self.error(
                                    "NXS034",
                                    "case arm cannot follow an `else` arm".to_string(),
                                    arm.span,
                                );
                            }
                            if !is_char {
                                if scrut_ty != Type::Error {
                                    self.error(
                                        "NXS030",
                                        "char patterns require a `char` scrutinee".to_string(),
                                        arm.span,
                                    );
                                }
                            } else if !seen_chars.insert(*value) {
                                self.error(
                                    "NXS032",
                                    format!("duplicate match arm for `{value:?}`"),
                                    arm.span,
                                );
                            }
                        }
                        nex_ast::Pattern::Str(value) => {
                            if else_seen {
                                self.error(
                                    "NXS034",
                                    "case arm cannot follow an `else` arm".to_string(),
                                    arm.span,
                                );
                            }
                            if !is_str {
                                if scrut_ty != Type::Error {
                                    self.error(
                                        "NXS030",
                                        "string patterns require a `string` scrutinee".to_string(),
                                        arm.span,
                                    );
                                }
                            } else if !seen_strs.insert(value.clone()) {
                                self.error(
                                    "NXS032",
                                    format!("duplicate match arm for `{value}`"),
                                    arm.span,
                                );
                            }
                        }
                    }
                    if let Some(guard) = &arm.guard {
                        let ty = self.infer(guard);
                        if ty != Type::Error && ty != Type::Bool {
                            self.error(
                                "NXS014",
                                format!("match guard must be `bool`, found `{ty}`"),
                                guard.span,
                            );
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

                if scrut_ty != Type::Error {
                    let exhaustive = else_seen
                        || (is_enum
                            && seen_variants.len()
                                == self
                                    .analysis
                                    .enums
                                    .get(if let Type::Enum(name) = scrut_ty {
                                        name
                                    } else {
                                        ""
                                    })
                                    .map(|info| info.variants.len())
                                    .unwrap_or(0))
                        || (is_bool && seen_bools.len() == 2);
                    if !exhaustive && !else_seen {
                        if is_enum {
                            self.warning(
                                "NXS033",
                                format!(
                                    "match does not cover all variants of enum `{}`",
                                    if let Type::Enum(name) = scrut_ty {
                                        name
                                    } else {
                                        ""
                                    }
                                ),
                                statement.span,
                            );
                        } else if is_int || is_char || is_str {
                            self.error(
                                "NXS033",
                                format!("match on `{scrut_ty}` must include an `else` arm"),
                                statement.span,
                            );
                        } else if is_bool {
                            self.warning(
                                "NXS033",
                                "match does not cover both `true` and `false`".to_string(),
                                statement.span,
                            );
                        }
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
            StmtKind::Unsafe(block) => {
                self.scopes.push(HashMap::new());
                self.check_block(block, false);
                self.scopes.pop();
            }
            StmtKind::Defer(block) => {
                self.scopes.push(HashMap::new());
                self.check_block(block, false);
                self.scopes.pop();
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

        // `var a: [i64] = [1, 2, 3]` — an array literal in a growable context
        // keeps the expected growable type instead of becoming `[i64; 3]`.
        if let ExprKind::Array(elements) = &expr.kind {
            if let Type::DynArray(display) = expected {
                let element = self
                    .analysis
                    .arrays
                    .get(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error);
                if element == Type::Error {
                    self.error(
                        "NXS035",
                        format!("unknown element type in `{expected}`"),
                        expr.span,
                    );
                    self.analysis.expr_types.insert(expr.span, Type::Error);
                    return Type::Error;
                }
                for item in elements {
                    self.check_expected(item, element);
                }
                self.analysis.expr_types.insert(expr.span, expected);
                return expected;
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

    /// Resolves the pointee type of a `Type::Ref` from its interned inner name.
    fn pointee_type(&self, ty: Type) -> Type {
        let Type::Ref { inner, .. } = ty else {
            return Type::Error;
        };
        if let Some(resolved) = Type::from_name(inner) {
            return resolved;
        }
        if let Some(info) = self.analysis.structs.get(inner) {
            return Type::Struct(info.name_static);
        }
        if let Some(info) = self.analysis.arrays.get(inner) {
            if info.len == 0 {
                return Type::DynArray(info.display);
            }
            return Type::Array {
                display: info.display,
                llvm: info.llvm,
            };
        }
        if self.analysis.enums.contains_key(inner) {
            if let Some(info) = self.analysis.enums.get(inner) {
                return Type::Enum(info.name_static);
            }
        }
        Type::Error
    }

    /// True when an expression denotes a place (something you can take an
    /// address of): a name, field, index, or dereference.
    fn is_place(&self, expr: &Expr) -> bool {
        matches!(
            expr.kind,
            ExprKind::Ident(_)
                | ExprKind::Field { .. }
                | ExprKind::Index { .. }
                | ExprKind::Deref(_)
        )
    }

    fn infer(&mut self, expr: &Expr) -> Type {
        let ty = match &expr.kind {
            ExprKind::Int(_) => Type::I64,
            ExprKind::Float(_) => Type::F64,
            ExprKind::Str(_) => Type::String,
            ExprKind::Char(_) => Type::Char,
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Try(inner) => self.infer_try(inner, expr.span),
            ExprKind::Borrow { mutable, place } => {
                let inner_ty = self.infer(place);
                if inner_ty == Type::Error || inner_ty == Type::Void {
                    return Type::Error;
                }
                if !self.is_place(place) {
                    self.error(
                        "NXS052",
                        "cannot borrow a non-place expression".to_string(),
                        expr.span,
                    );
                    return Type::Error;
                }
                Type::Ref {
                    inner: inner_ty.name(),
                    mutable: *mutable,
                }
            }
            ExprKind::Deref(inner) => {
                let ref_ty = self.infer(inner);
                match ref_ty {
                    Type::Ref { inner: name, .. } => self.pointee_type(Type::Ref {
                        inner: name,
                        mutable: false,
                    }),
                    Type::Error => Type::Error,
                    _ => {
                        self.error(
                            "NXS051",
                            format!("cannot dereference a value of type `{ref_ty}`"),
                            expr.span,
                        );
                        Type::Error
                    }
                }
            }
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
                        nex_ast::UnaryOp::BitNot => {
                            if operand_ty.is_integer() {
                                operand_ty
                            } else {
                                self.error(
                                    "NXS008",
                                    format!("`~` requires an integer, found `{operand_ty}`"),
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
                        } else if *op == BinOp::Add && ty == Type::String {
                            // `+` concatenates strings (sugar for `concat`).
                            Type::String
                        } else {
                            self.error(
                                "NXS008",
                                format!("arithmetic is not defined for type `{ty}`"),
                                expr.span,
                            );
                            Type::Error
                        }
                    }
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                        let Some(ty) = self.unify_operands(left, right) else {
                            return Type::Error;
                        };
                        if ty.is_integer() {
                            ty
                        } else {
                            self.error(
                                "NXS008",
                                format!("bitwise operators require integer operands, found `{ty}`"),
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
                                format!("equality is not implemented for type `{ty}`"),
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
                                format!("ordering is not defined for type `{ty}`"),
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
                        "empty array literals require a type annotation, e.g. `[i64]`".to_string(),
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
                } else if let Type::DynArray(display) = base_ty {
                    self.analysis
                        .arrays
                        .get(display)
                        .map(|info| info.elem)
                        .unwrap_or(Type::Error)
                } else if base_ty == Type::String {
                    Type::String
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
                        "empty tuples are not supported".to_string(),
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
                                    type_params: Vec::new(),
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

    /// Resolve `receiver.method(args)` when `receiver` is a local (or `self`)
    /// whose struct type has a method registered by an `impl` block.
    fn check_method_call(&mut self, callee: &str, args: &[Expr], span: Span) -> Option<Type> {
        let (receiver, method_name) = callee.rsplit_once('.')?;
        if receiver.is_empty() || method_name.is_empty() {
            return None;
        }

        let root = receiver.split('.').next().unwrap_or(receiver);
        let receiver_var = self.lookup(root)?;

        if receiver.contains('.') {
            self.error(
                "NXS047",
                "method calls on nested fields are not supported yet; assign the field to a local first"
                    .to_string(),
                span,
            );
            for arg in args {
                self.infer(arg);
            }
            return Some(Type::Error);
        }

        let receiver_ty = receiver_var.ty;
        if receiver_ty == Type::Error {
            for arg in args {
                self.infer(arg);
            }
            return Some(Type::Error);
        }

        let symbol = match receiver_ty {
            Type::Struct(name) => format!("{name}.{method_name}"),
            _ => {
                self.error(
                    "NXS046",
                    format!("type `{receiver_ty}` has no method `{method_name}`"),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Some(Type::Error);
            }
        };

        let Some(signature) = self.analysis.functions.get(&symbol).cloned() else {
            self.error(
                "NXS046",
                format!("type `{receiver_ty}` has no method `{method_name}`"),
                span,
            );
            for arg in args {
                self.infer(arg);
            }
            return Some(Type::Error);
        };

        let expected = &signature.params[1..];
        if args.len() != expected.len() {
            self.error(
                "NXS004",
                format!(
                    "method `{method_name}` expects {} argument(s), found {}",
                    expected.len(),
                    args.len()
                ),
                span,
            );
            for arg in args {
                self.infer(arg);
            }
            return Some(Type::Error);
        }
        for (arg, expected_ty) in args.iter().zip(expected.iter()) {
            self.check_expected(arg, *expected_ty);
        }
        self.analysis.method_calls.insert(
            span,
            MethodCallInfo {
                symbol,
                receiver: root.to_string(),
                receiver_ty,
                ret: signature.ret,
            },
        );
        Some(signature.ret)
    }

    fn infer_try(&mut self, expr: &Expr, span: Span) -> Type {
        let scrut_ty = self.infer(expr);
        if scrut_ty == Type::Error {
            return Type::Error;
        }
        if let Some(value_ty) = self.option_value_type(&scrut_ty) {
            if let Some(current) = &self.current_fn {
                if let Some(ret_value) = self.option_value_type(&current.ret) {
                    if ret_value != value_ty {
                        self.error(
                            "NXS004",
                            format!(
                                "`?` requires matching Option types: found `{value_ty}`, function returns `{ret_value}`"
                            ),
                            span,
                        );
                        return Type::Error;
                    }
                } else {
                    self.error(
                        "NXS004",
                        "`?` requires the function to return `Option<T>`".to_string(),
                        span,
                    );
                    return Type::Error;
                }
            } else {
                self.error(
                    "NXS004",
                    "`?` can only be used inside a function".to_string(),
                    span,
                );
                return Type::Error;
            }
            return value_ty;
        }
        if let Some((value_ty, err_ty)) = self.result_types(&scrut_ty) {
            if let Some(current) = &self.current_fn {
                if let Some((ret_value, ret_err)) = self.result_types(&current.ret) {
                    if ret_value != value_ty || ret_err != err_ty {
                        self.error(
                            "NXS004",
                            format!(
                                "`?` requires matching Result types: found `Result<{value_ty}, {err_ty}>`, function returns `Result<{ret_value}, {ret_err}>`"
                            ),
                            span,
                        );
                        return Type::Error;
                    }
                } else {
                    self.error(
                        "NXS004",
                        "`?` requires the function to return `Result<T, E>`".to_string(),
                        span,
                    );
                    return Type::Error;
                }
            } else {
                self.error(
                    "NXS004",
                    "`?` can only be used inside a function".to_string(),
                    span,
                );
                return Type::Error;
            }
            return value_ty;
        }
        self.error(
            "NXS004",
            format!("`?` requires `Option<T>` or `Result<T, E>`, found `{scrut_ty}`"),
            span,
        );
        Type::Error
    }

    fn option_value_type(&self, ty: &Type) -> Option<Type> {
        let Type::Struct(name) = ty else {
            return None;
        };
        let info = self.analysis.struct_info(name)?;
        if info.fields.len() != 2 {
            return None;
        }
        if info.fields[0].0 != "has" || info.fields[1].0 != "value" {
            return None;
        }
        if info.fields[0].1 != Type::Bool {
            return None;
        }
        Some(info.fields[1].1)
    }

    fn result_types(&self, ty: &Type) -> Option<(Type, Type)> {
        let Type::Struct(name) = ty else {
            return None;
        };
        let info = self.analysis.struct_info(name)?;
        if info.fields.len() != 3 {
            return None;
        }
        if info.fields[0].0 != "ok" || info.fields[1].0 != "value" || info.fields[2].0 != "error" {
            return None;
        }
        if info.fields[0].1 != Type::Bool {
            return None;
        }
        Some((info.fields[1].1, info.fields[2].1))
    }

    fn check_call(&mut self, callee: &str, callee_span: Span, args: &[Expr], span: Span) -> Type {
        if let Some(result) = self.check_method_call(callee, args, span) {
            return result;
        }
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
            if arg_ty == Type::String || matches!(arg_ty, Type::Array { .. } | Type::DynArray(_)) {
                return Type::I64;
            }
            self.error(
                "NXS004",
                format!("`len` expects a `string` or array, found `{arg_ty}`"),
                args[0].span,
            );
            return Type::Error;
        }

        if callee == "clone" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`clone` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let ty = self.infer(&args[0]);
            if ty == Type::Void {
                self.error("NXS004", "`clone` cannot copy a void value", args[0].span);
                return Type::Error;
            }
            return ty;
        }

        if callee == "drop" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`drop` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Void;
            }
            let ty = self.infer(&args[0]);
            if !matches!(ty, Type::DynArray(_)) {
                self.error(
                    "NXS004",
                    format!("`drop` expects a growable array, found `{ty}`"),
                    args[0].span,
                );
            }
            return Type::Void;
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
            if arg_ty == Type::I64 || arg_ty == Type::F64 {
                return arg_ty;
            }
            if arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`abs` expects `i64` or `f64`, found `{arg_ty}`"),
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
            if args.is_empty() || args.len() > 2 {
                self.error(
                    "NXS004",
                    format!("`assert` expects 1 or 2 arguments, found {}", args.len()),
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
            if let Some(message) = args.get(1) {
                let message_ty = self.infer(message);
                if message_ty != Type::String && message_ty != Type::Error {
                    self.error(
                        "NXS004",
                        format!("`assert` message must be `string`, found `{message_ty}`"),
                        message.span,
                    );
                }
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

        if callee == "arena_new" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`arena_new` expects 1 argument, found {}", args.len()),
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
                    format!("`arena_new` expects `i64`, found `{arg_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "arena_alloc" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`arena_alloc` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let a = self.infer(&args[0]);
            let s = self.infer(&args[1]);
            if a != Type::I64 && a != Type::Error {
                self.error(
                    "NXS004",
                    format!("`arena_alloc` expects `i64` handle, found `{a}`"),
                    args[0].span,
                );
            }
            if s != Type::I64 && s != Type::Error {
                self.error(
                    "NXS004",
                    format!("`arena_alloc` expects `i64` size, found `{s}`"),
                    args[1].span,
                );
            }
            return Type::I64;
        }

        if callee == "arena_reset" || callee == "arena_free" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Void;
            }
            let a = self.infer(&args[0]);
            if a != Type::I64 && a != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects `i64` handle, found `{a}`"),
                    args[0].span,
                );
            }
            return Type::Void;
        }

        if callee == "arena_used" || callee == "arena_capacity" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::I64;
            }
            let a = self.infer(&args[0]);
            if a != Type::I64 && a != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects `i64` handle, found `{a}`"),
                    args[0].span,
                );
            }
            return Type::I64;
        }

        if callee == "str_split" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`str_split` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let text_ty = self.infer(&args[0]);
            let sep_ty = self.infer(&args[1]);
            if text_ty != Type::String && text_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`str_split` expects `string`, found `{text_ty}`"),
                    args[0].span,
                );
            }
            if sep_ty != Type::String && sep_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`str_split` expects `string`, found `{sep_ty}`"),
                    args[1].span,
                );
            }
            // register [string] dyn array
            let display = intern_dyn_array_type(Type::String);
            if !self.analysis.arrays.contains_key(display) {
                self.analysis.arrays.insert(
                    display.to_string(),
                    ArrayInfo {
                        display,
                        llvm: DYN_ARRAY_LLVM,
                        elem: Type::String,
                        len: 0,
                    },
                );
            }
            return Type::DynArray(display);
        }

        if callee == "slice" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!("`slice` expects 3 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let base_ty = self.infer(&args[0]);
            let start_ty = self.infer(&args[1]);
            let end_ty = self.infer(&args[2]);
            if start_ty != Type::I64 && start_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`slice` expects `i64` start, found `{start_ty}`"),
                    args[1].span,
                );
            }
            if end_ty != Type::I64 && end_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`slice` expects `i64` end, found `{end_ty}`"),
                    args[2].span,
                );
            }
            match base_ty {
                Type::String => return Type::String,
                Type::Array { display, .. } | Type::DynArray(display) => {
                    let info = self.analysis.array_info(display).cloned();
                    let elem_ty = info.map(|i| i.elem).unwrap_or(Type::Error);
                    if elem_ty == Type::Error {
                        return Type::Error;
                    }
                    let out_display = if matches!(base_ty, Type::DynArray(_)) {
                        intern_dyn_array_type(elem_ty)
                    } else {
                        // fixed array -> growable slice
                        intern_dyn_array_type(elem_ty)
                    };
                    if !self.analysis.arrays.contains_key(out_display) {
                        self.analysis.arrays.insert(
                            out_display.to_string(),
                            ArrayInfo {
                                display: out_display,
                                llvm: DYN_ARRAY_LLVM,
                                elem: elem_ty,
                                len: 0,
                            },
                        );
                    }
                    return Type::DynArray(out_display);
                }
                _ => {
                    if base_ty != Type::Error {
                        self.error(
                            "NXS004",
                            format!("`slice` expects a string or array, found `{base_ty}`"),
                            args[0].span,
                        );
                    }
                    return Type::Error;
                }
            }
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

        if callee == "str_index_of_from" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!(
                        "`str_index_of_from` expects 3 arguments, found {}",
                        args.len()
                    ),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let value_ty = self.infer(&args[0]);
            let needle_ty = self.infer(&args[1]);
            let start_ty = self.infer(&args[2]);
            if (value_ty != Type::String || needle_ty != Type::String || start_ty != Type::I64)
                && (value_ty != Type::Error && needle_ty != Type::Error && start_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    "`str_index_of_from` expects `(string, string, i64)`",
                    span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "char_at" || callee == "char_code" {
            if callee == "char_at" && args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`char_at` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            if callee == "char_code" && args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`char_code` expects 1 argument, found {}", args.len()),
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
            if callee == "char_at" {
                let index_ty = self.infer(&args[1]);
                if index_ty != Type::I64 && index_ty != Type::Error {
                    self.error("NXS004", "`char_at` expects an `i64` index", span);
                    return Type::Error;
                }
                return Type::String;
            }
            return Type::I64;
        }

        if callee == "unix_time" || callee == "now_ms" || callee == "http_date" {
            if !args.is_empty() {
                self.error("NXS004", format!("`{callee}` takes no arguments"), span);
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            return if callee == "http_date" {
                Type::String
            } else {
                Type::I64
            };
        }

        if callee == "sleep_ms" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`sleep_ms` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let duration_ty = self.infer(&args[0]);
            if duration_ty != Type::I64 && duration_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`sleep_ms` expects an `i64`, found `{duration_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::Void;
        }

        if callee == "http_response_keepalive" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!(
                        "`http_response_keepalive` expects 3 arguments, found {}",
                        args.len()
                    ),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let status_ty = self.infer(&args[0]);
            let type_ty = self.infer(&args[1]);
            let body_ty = self.infer(&args[2]);
            let all_strings =
                status_ty == Type::String && type_ty == Type::String && body_ty == Type::String;
            if !all_strings
                && status_ty != Type::Error
                && type_ty != Type::Error
                && body_ty != Type::Error
            {
                self.error(
                    "NXS004",
                    "`http_response_keepalive` expects `(string, string, string)`",
                    span,
                );
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "http_method"
            || callee == "http_path"
            || callee == "http_version"
            || callee == "http_body"
        {
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
            let request_ty = self.infer(&args[0]);
            if request_ty != Type::String && request_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a `string` request, found `{request_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "http_header" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`http_header` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let request_ty = self.infer(&args[0]);
            let name_ty = self.infer(&args[1]);
            if (request_ty != Type::String || name_ty != Type::String)
                && (request_ty != Type::Error && name_ty != Type::Error)
            {
                self.error("NXS004", "`http_header` expects `(string, string)`", span);
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "http_response" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!("`http_response` expects 3 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let status_ty = self.infer(&args[0]);
            let type_ty = self.infer(&args[1]);
            let body_ty = self.infer(&args[2]);
            if (status_ty != Type::String || type_ty != Type::String || body_ty != Type::String)
                && (status_ty != Type::Error && type_ty != Type::Error && body_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    "`http_response` expects `(string, string, string)`",
                    span,
                );
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "tcp_connect" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`tcp_connect` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let host_ty = self.infer(&args[0]);
            let port_ty = self.infer(&args[1]);
            if (host_ty != Type::String || port_ty != Type::I64)
                && (host_ty != Type::Error && port_ty != Type::Error)
            {
                self.error("NXS004", "`tcp_connect` expects `(string, i64)`", span);
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "tcp_listen" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`tcp_listen` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let port_ty = self.infer(&args[0]);
            let backlog_ty = self.infer(&args[1]);
            if (port_ty != Type::I64 || backlog_ty != Type::I64)
                && (port_ty != Type::Error && backlog_ty != Type::Error)
            {
                self.error("NXS004", "`tcp_listen` expects `(i64, i64)`", span);
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "tcp_accept" || callee == "tcp_close" {
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
            let fd_ty = self.infer(&args[0]);
            if fd_ty != Type::I64 && fd_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects an `i64` handle, found `{fd_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return if callee == "tcp_accept" {
                Type::I64
            } else {
                Type::Bool
            };
        }

        if callee == "tcp_write" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!("`tcp_write` expects 3 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let fd_ty = self.infer(&args[0]);
            let data_ty = self.infer(&args[1]);
            let length_ty = self.infer(&args[2]);
            if (fd_ty != Type::I64 || data_ty != Type::String || length_ty != Type::I64)
                && (fd_ty != Type::Error && data_ty != Type::Error && length_ty != Type::Error)
            {
                self.error("NXS004", "`tcp_write` expects `(i64, string, i64)`", span);
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "tcp_read" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`tcp_read` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let fd_ty = self.infer(&args[0]);
            let max_ty = self.infer(&args[1]);
            if (fd_ty != Type::I64 || max_ty != Type::I64)
                && (fd_ty != Type::Error && max_ty != Type::Error)
            {
                self.error("NXS004", "`tcp_read` expects `(i64, i64)`", span);
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "read_file" || callee == "file_exists" || callee == "file_size" {
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
                    format!("`{callee}` expects a `string` path, found `{value_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return match callee {
                "read_file" => Type::String,
                "file_size" => Type::I64,
                _ => Type::Bool,
            };
        }

        if callee == "write_file" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`write_file` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let path_ty = self.infer(&args[0]);
            let content_ty = self.infer(&args[1]);
            if (path_ty != Type::String || content_ty != Type::String)
                && (path_ty != Type::Error && content_ty != Type::Error)
            {
                self.error("NXS004", "`write_file` expects `(string, string)`", span);
                return Type::Error;
            }
            if path_ty == Type::Error || content_ty == Type::Error {
                return Type::Error;
            }
            return Type::Bool;
        }

        if callee == "file_append" || callee == "file_copy" || callee == "file_rename" {
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
            let a = self.infer(&args[0]);
            let b = self.infer(&args[1]);
            if (a != Type::String || b != Type::String) && (a != Type::Error && b != Type::Error) {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects `(string, string)`"),
                    span,
                );
                return Type::Error;
            }
            return Type::Bool;
        }

        if callee == "file_delete" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`file_delete` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let p = self.infer(&args[0]);
            if p != Type::String && p != Type::Error {
                self.error(
                    "NXS004",
                    format!("`file_delete` expects `string`, found `{p}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::Bool;
        }

        if callee == "json_parse" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`json_parse` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let text_ty = self.infer(&args[0]);
            if text_ty != Type::String && text_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`json_parse` expects a `string`, found `{text_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "json_type"
            || callee == "json_len"
            || callee == "json_str"
            || callee == "json_num"
            || callee == "json_bool"
            || callee == "json_is_null"
        {
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
            let handle_ty = self.infer(&args[0]);
            if handle_ty != Type::I64 && handle_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a JSON handle, found `{handle_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return match callee {
                "json_type" | "json_str" => Type::String,
                "json_num" => Type::F64,
                "json_bool" | "json_is_null" => Type::Bool,
                _ => Type::I64,
            };
        }

        if callee == "json_at" || callee == "json_get" {
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
            let handle_ty = self.infer(&args[0]);
            let second_ty = self.infer(&args[1]);
            let second_ok = if callee == "json_at" {
                second_ty == Type::I64
            } else {
                second_ty == Type::String
            };
            if (handle_ty != Type::I64 || !second_ok)
                && (handle_ty != Type::Error && second_ty != Type::Error)
            {
                self.error(
                    "NXS004",
                    format!(
                        "`{callee}` expects `(i64, {})`",
                        if callee == "json_at" { "i64" } else { "string" }
                    ),
                    span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "str_split_count" || callee == "str_split_at" {
            let expected_args = if callee == "str_split_count" { 2 } else { 3 };
            if args.len() != expected_args {
                self.error(
                    "NXS004",
                    format!(
                        "`{callee}` expects {expected_args} arguments, found {}",
                        args.len()
                    ),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let text_ty = self.infer(&args[0]);
            let sep_ty = self.infer(&args[1]);
            let index_ty = if callee == "str_split_at" {
                self.infer(&args[2])
            } else {
                Type::I64
            };
            let ok = text_ty == Type::String
                && sep_ty == Type::String
                && (callee == "str_split_count" || index_ty == Type::I64);
            if !ok && text_ty != Type::Error && sep_ty != Type::Error && index_ty != Type::Error {
                self.error(
                    "NXS004",
                    "`str_split_at` expects `(string, string, i64)`",
                    span,
                );
                return Type::Error;
            }
            return if callee == "str_split_count" {
                Type::I64
            } else {
                Type::String
            };
        }

        if callee == "json_stringify" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`json_stringify` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let handle_ty = self.infer(&args[0]);
            if handle_ty != Type::I64 && handle_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`json_stringify` expects a JSON handle, found `{handle_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "map_new" {
            if !args.is_empty() {
                self.error("NXS004", "`map_new` takes no arguments", span);
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "map_len" || callee == "map_key_at" {
            let expected = if callee == "map_len" { 1 } else { 2 };
            if args.len() != expected {
                self.error(
                    "NXS004",
                    format!(
                        "`{callee}` expects {expected} arguments, found {}",
                        args.len()
                    ),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let map_ty = self.infer(&args[0]);
            if map_ty != Type::I64 && map_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a map handle, found `{map_ty}`"),
                    span,
                );
                return Type::Error;
            }
            if callee == "map_key_at" {
                let index_ty = self.infer(&args[1]);
                if index_ty != Type::I64 && index_ty != Type::Error {
                    self.error("NXS004", "`map_key_at` expects an `i64` index", span);
                    return Type::Error;
                }
                return Type::String;
            }
            return Type::I64;
        }

        if callee == "map_set"
            || callee == "map_get"
            || callee == "map_has"
            || callee == "map_remove"
        {
            let expected = if callee == "map_set" { 3 } else { 2 };
            if args.len() != expected {
                self.error(
                    "NXS004",
                    format!(
                        "`{callee}` expects {expected} arguments, found {}",
                        args.len()
                    ),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let map_ty = self.infer(&args[0]);
            let key_ty = self.infer(&args[1]);
            let value_ok = if callee == "map_set" {
                let value_ty = self.infer(&args[2]);
                value_ty == Type::String || value_ty == Type::Error
            } else {
                true
            };
            let types_ok = map_ty == Type::I64
                && key_ty == Type::String
                && value_ok
                && (map_ty != Type::Error && key_ty != Type::Error);
            if !types_ok {
                self.error("NXS004", "`map_set` expects `(i64, string, string)`", span);
                return Type::Error;
            }
            return match callee {
                "map_get" => Type::String,
                "map_has" | "map_remove" => Type::Bool,
                _ => Type::Bool,
            };
        }

        if callee == "thread_spawn" {
            if args.is_empty() || args.len() > 2 {
                self.error("NXS004", "`thread_spawn` expects 1 or 2 arguments", span);
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let name_ty = self.infer(&args[0]);
            if name_ty != Type::String && name_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`thread_spawn` expects a function name, found `{name_ty}`"),
                    span,
                );
                return Type::Error;
            }
            if let ExprKind::Str(name) = &args[0].kind {
                if !self.analysis.functions.contains_key(name) {
                    self.error("NXS004", format!("unknown function `{name}`"), args[0].span);
                    return Type::Error;
                }
            }
            if args.len() == 2 {
                let arg_ty = self.infer(&args[1]);
                if arg_ty != Type::I64 && arg_ty != Type::Error {
                    self.error(
                        "NXS004",
                        format!("thread argument must be `i64`, found `{arg_ty}`"),
                        span,
                    );
                    return Type::Error;
                }
            }
            return Type::I64;
        }

        if callee == "thread_join" || callee == "thread_running" {
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
            let handle_ty = self.infer(&args[0]);
            if handle_ty != Type::I64 && handle_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a thread handle, found `{handle_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::Bool;
        }

        if callee == "mutex_new" {
            if !args.is_empty() {
                self.error("NXS004", "`mutex_new` takes no arguments", span);
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "mutex_lock" || callee == "mutex_unlock" {
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
            let handle_ty = self.infer(&args[0]);
            if handle_ty != Type::I64 && handle_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a mutex handle, found `{handle_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::Void;
        }

        if callee == "router_new" {
            if !args.is_empty() {
                self.error("NXS004", "`router_new` takes no arguments", span);
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "router_add" || callee == "router_match" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects 3 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let router_ty = self.infer(&args[0]);
            let a_ty = self.infer(&args[1]);
            let b_ty = self.infer(&args[2]);
            let ok = router_ty == Type::I64 && a_ty == Type::String && b_ty == Type::String;
            if !ok && router_ty != Type::Error && a_ty != Type::Error && b_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects `(i64, string, string)`"),
                    span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "router_param" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`router_param` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let router_ty = self.infer(&args[0]);
            let name_ty = self.infer(&args[1]);
            if (router_ty != Type::I64 || name_ty != Type::String)
                && (router_ty != Type::Error && name_ty != Type::Error)
            {
                self.error("NXS004", "`router_param` expects `(i64, string)`", span);
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "pool_new" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`pool_new` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let workers = self.infer(&args[0]);
            let capacity = self.infer(&args[1]);
            if (workers != Type::I64 || capacity != Type::I64)
                && (workers != Type::Error && capacity != Type::Error)
            {
                self.error(
                    "NXS004",
                    "`pool_new` expects `(workers: i64, capacity: i64)`",
                    span,
                );
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "pool_submit" {
            if args.len() != 3 {
                self.error(
                    "NXS004",
                    format!("`pool_submit` expects 3 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let pool_ty = self.infer(&args[0]);
            let name_ty = self.infer(&args[1]);
            let arg_ty = self.infer(&args[2]);
            if (pool_ty != Type::I64 || name_ty != Type::String || arg_ty != Type::I64)
                && (pool_ty != Type::Error && name_ty != Type::Error && arg_ty != Type::Error)
            {
                self.error("NXS004", "`pool_submit` expects `(i64, string, i64)`", span);
                return Type::Error;
            }
            if let ExprKind::Str(name) = &args[1].kind {
                if !self.analysis.functions.contains_key(name) {
                    self.error("NXS004", format!("unknown function `{name}`"), args[1].span);
                    return Type::Error;
                }
            }
            return Type::Bool;
        }

        if callee == "pool_workers" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`pool_workers` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "floor" || callee == "ceil" || callee == "round" || callee == "trunc" {
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
            if value_ty != Type::F64 && value_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects an `f64`, found `{value_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::F64;
        }

        if callee == "json_escape" || callee == "json_quote" {
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
            return Type::String;
        }

        if callee == "builder_new" {
            if !args.is_empty() {
                self.error("NXS004", "`builder_new` takes no arguments", span);
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "builder_len" || callee == "builder_string" || callee == "builder_clear" {
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
            let handle_ty = self.infer(&args[0]);
            if handle_ty != Type::I64 && handle_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a builder handle, found `{handle_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return match callee {
                "builder_string" => Type::String,
                "builder_clear" => Type::Void,
                _ => Type::I64,
            };
        }

        if callee == "builder_append" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`builder_append` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let handle_ty = self.infer(&args[0]);
            let text_ty = self.infer(&args[1]);
            if (handle_ty != Type::I64 || text_ty != Type::String)
                && (handle_ty != Type::Error && text_ty != Type::Error)
            {
                self.error("NXS004", "`builder_append` expects `(i64, string)`", span);
                return Type::Error;
            }
            return Type::Bool;
        }

        if callee == "pop" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`pop` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let arg_ty = self.infer(&args[0]);
            if let Type::DynArray(display) = arg_ty {
                if let Some(info) = self.analysis.array_info(display) {
                    return info.elem;
                }
            }
            if arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`pop` expects a growable array, found `{arg_ty}`"),
                    args[0].span,
                );
            }
            return Type::Error;
        }

        if callee == "contains" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`contains` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            let value_ty = self.infer(&args[1]);
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`contains` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
            }
            if elem_ty != Type::Error && value_ty != Type::Error && elem_ty != value_ty {
                self.error(
                    "NXS004",
                    format!("`contains` expects `{elem_ty}`, found `{value_ty}`"),
                    args[1].span,
                );
            }
            return Type::Bool;
        }

        if callee == "index_of" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`index_of` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            let value_ty = self.infer(&args[1]);
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`index_of` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
            }
            if elem_ty != Type::Error && value_ty != Type::Error && elem_ty != value_ty {
                self.error(
                    "NXS004",
                    format!("`index_of` expects `{elem_ty}`, found `{value_ty}`"),
                    args[1].span,
                );
            }
            return Type::I64;
        }

        if callee == "reverse" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`reverse` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Void;
            }
            let arg_ty = self.infer(&args[0]);
            match arg_ty {
                Type::Array { .. } | Type::DynArray(_) => {}
                _ => {
                    if arg_ty != Type::Error {
                        self.error(
                            "NXS004",
                            format!("`reverse` expects an array, found `{arg_ty}`"),
                            args[0].span,
                        );
                    }
                }
            }
            return Type::Void;
        }

        if callee == "sort" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`sort` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Void;
            }
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`sort` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
            } else if elem_ty != Type::Error
                && !matches!(elem_ty, Type::I64 | Type::F64 | Type::String)
            {
                self.error(
                    "NXS004",
                    format!(
                        "`sort` supports arrays of `i64`, `f64` or `string`, found `{elem_ty}`"
                    ),
                    args[0].span,
                );
            }
            return Type::Void;
        }

        if callee == "join" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`join` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`join` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
            } else if elem_ty != Type::Error && elem_ty != Type::String {
                self.error(
                    "NXS004",
                    format!("`join` expects a `[string]` array, found element `{elem_ty}`"),
                    args[0].span,
                );
            }
            let separator_ty = self.infer(&args[1]);
            if separator_ty != Type::String && separator_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`join` expects a `string` separator, found `{separator_ty}`"),
                    args[1].span,
                );
            }
            return Type::String;
        }

        if callee == "sum_of" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`sum_of` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`sum_of` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            if elem_ty != Type::Error && !matches!(elem_ty, Type::I64 | Type::F64) {
                self.error(
                    "NXS004",
                    format!("`sum_of` supports `[i64]` and `[f64]`, found element `{elem_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return elem_ty;
        }

        if callee == "product" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`product` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`product` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            if elem_ty != Type::Error && !matches!(elem_ty, Type::I64 | Type::F64) {
                self.error(
                    "NXS004",
                    format!("`product` supports `[i64]` and `[f64]`, found element `{elem_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return elem_ty;
        }

        if callee == "min_of" || callee == "max_of" {
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
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            if elem_ty != Type::Error && !matches!(elem_ty, Type::I64 | Type::F64) {
                self.error(
                    "NXS004",
                    format!("`{callee}` supports `[i64]` and `[f64]`, found element `{elem_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return elem_ty;
        }

        if callee == "count" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`count` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::I64;
            }
            let array_ty = self.infer(&args[0]);
            let elem_ty = match array_ty {
                Type::Array { display, .. } | Type::DynArray(display) => self
                    .analysis
                    .array_info(display)
                    .map(|info| info.elem)
                    .unwrap_or(Type::Error),
                _ => Type::Error,
            };
            if array_ty != Type::Error && elem_ty == Type::Error {
                self.error(
                    "NXS004",
                    format!("`count` expects an array, found `{array_ty}`"),
                    args[0].span,
                );
            }
            let value_ty = self.infer(&args[1]);
            if elem_ty != Type::Error && value_ty != Type::Error && elem_ty != value_ty {
                self.error(
                    "NXS004",
                    format!("`count` expects a value of type `{elem_ty}`, found `{value_ty}`"),
                    args[1].span,
                );
            }
            if elem_ty != Type::Error
                && !matches!(
                    elem_ty,
                    Type::I64 | Type::F64 | Type::String | Type::Bool | Type::Char
                )
            {
                self.error(
                    "NXS004",
                    format!(
                        "`count` supports `i64`, `f64`, `string`, `bool`, `char`, found `{elem_ty}`"
                    ),
                    args[0].span,
                );
            }
            return Type::I64;
        }

        if callee == "push" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!("`push` expects 2 arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let array_ty = self.infer(&args[0]);
            let value_ty = self.infer(&args[1]);
            return match array_ty {
                Type::DynArray(display) => {
                    let element = self
                        .analysis
                        .arrays
                        .get(display)
                        .map(|info| info.elem)
                        .unwrap_or(Type::Error);
                    if element == Type::Error || value_ty == Type::Error {
                        return Type::Error;
                    }
                    if element != value_ty {
                        self.error(
                            "NXS004",
                            format!("`push` expects `{element}`, found `{value_ty}`"),
                            args[1].span,
                        );
                        return Type::Error;
                    }
                    Type::Void
                }
                _ => {
                    self.error(
                        "NXS004",
                        format!("`push` expects a growable array, found `{array_ty}`"),
                        args[0].span,
                    );
                    Type::Error
                }
            };
        }

        if callee == "tcp_set_timeout" {
            if args.len() != 2 {
                self.error(
                    "NXS004",
                    format!(
                        "`tcp_set_timeout` expects 2 arguments, found {}",
                        args.len()
                    ),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let socket_ty = self.infer(&args[0]);
            let timeout_ty = self.infer(&args[1]);
            if (socket_ty != Type::I64 || timeout_ty != Type::I64)
                && (socket_ty != Type::Error && timeout_ty != Type::Error)
            {
                self.error("NXS004", "`tcp_set_timeout` expects `(i64, i64)`", span);
                return Type::Error;
            }
            return Type::Bool;
        }

        if callee == "str_reverse" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`str_reverse` expects 1 argument, found {}", args.len()),
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
                    format!("`str_reverse` expects a `string`, found `{value_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return value_ty;
        }

        if callee == "parse_i64" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`parse_i64` expects 1 argument, found {}", args.len()),
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
                    format!("`parse_i64` expects a `string`, found `{value_ty}`"),
                    span,
                );
                return Type::Error;
            }
            return Type::I64;
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
            if arg_ty == Type::I64
                || arg_ty == Type::F64
                || arg_ty == Type::Bool
                || arg_ty == Type::String
            {
                return Type::String;
            }
            if arg_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!(
                        "`to_string` expects `i64`, `f64`, `bool` or `string`, found `{arg_ty}`"
                    ),
                    args[0].span,
                );
            }
            return Type::Error;
        }

        if callee == "env" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`env` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let name_ty = self.infer(&args[0]);
            if name_ty != Type::String && name_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`env` expects a `string` name, found `{name_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "args_count" {
            if !args.is_empty() {
                self.error(
                    "NXS004",
                    format!("`args_count` expects no arguments, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            return Type::I64;
        }

        if callee == "arg_at" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`arg_at` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let index_ty = self.infer(&args[0]);
            if index_ty != Type::I64 && index_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`arg_at` expects an `i64` index, found `{index_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::String;
        }

        if callee == "exit" {
            if args.len() != 1 {
                self.error(
                    "NXS004",
                    format!("`exit` expects 1 argument, found {}", args.len()),
                    span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            let code_ty = self.infer(&args[0]);
            if code_ty != Type::I64 && code_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`exit` expects an `i64` status code, found `{code_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::Void;
        }

        if callee == "log_info" || callee == "log_warn" || callee == "log_error" {
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
            let message_ty = self.infer(&args[0]);
            if message_ty != Type::String && message_ty != Type::Error {
                self.error(
                    "NXS004",
                    format!("`{callee}` expects a `string` message, found `{message_ty}`"),
                    args[0].span,
                );
                return Type::Error;
            }
            return Type::Void;
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
            // Generic struct monomorphization.
            if !info.type_params.is_empty() {
                let arg_types: Vec<Type> = args.iter().map(|a| self.infer(a)).collect();
                let mut mapping: std::collections::HashMap<String, Type> =
                    std::collections::HashMap::new();
                for ((_, field_ty), arg_ty) in info.fields.iter().zip(arg_types.iter()) {
                    if let Type::TypeParam(name) = field_ty {
                        mapping.insert(name.to_string(), *arg_ty);
                    }
                }
                let mut concrete_names = Vec::new();
                for tp in &info.type_params {
                    let ct = mapping.get(tp).copied().unwrap_or(Type::Error);
                    concrete_names.push(ct.name().to_string());
                }
                let mono_name = format!("{}_{}", callee, concrete_names.join("_"));
                self.instantiate_generic_struct(callee, &concrete_names, &mapping);
                // check args against mono struct
                let mono_info = self
                    .analysis
                    .structs
                    .get(&mono_name)
                    .cloned()
                    .unwrap_or(info.clone());
                if args.len() != mono_info.fields.len() {
                    self.error(
                        "NXS004",
                        format!(
                            "struct `{mono_name}` expects {} field value(s), found {}",
                            mono_info.fields.len(),
                            args.len()
                        ),
                        span,
                    );
                    for arg in args {
                        self.infer(arg);
                    }
                    return Type::Error;
                }
                for (index, arg) in args.iter().enumerate() {
                    if let Some((_, ty)) = mono_info.fields.get(index) {
                        self.check_expected(arg, *ty);
                    } else {
                        self.infer(arg);
                    }
                }
                self.analysis.mono_structs.insert(span, mono_name.clone());
                return Type::Struct(self.analysis.structs.get(&mono_name).unwrap().name_static);
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

        if sig.type_params.is_empty() {
            // normal
        } else {
            // monomorphize
            let arg_types: Vec<Type> = args.iter().map(|a| self.infer(a)).collect();
            // map type params to concrete types: assum_ofe one type param, infer from first param type
            // Simple: type param -> type of first argument if first param uses it.
            let mut mapping: std::collections::HashMap<String, Type> =
                std::collections::HashMap::new();
            for (param_ty, arg_ty) in sig.params.iter().zip(arg_types.iter()) {
                if let Type::TypeParam(name) = param_ty {
                    mapping.insert(name.to_string(), *arg_ty);
                }
            }
            // build mono name
            let mut concrete_names = Vec::new();
            for tp in &sig.type_params {
                let ct = mapping.get(tp).copied().unwrap_or(Type::Error);
                concrete_names.push(ct.name());
            }
            let mono_name = format!("{}_{}", callee, concrete_names.join("_"));
            if !self.analysis.functions.contains_key(&mono_name) {
                let program_functions = self.program.functions.clone();
                let generic_fn = program_functions.iter().find(|f| f.name == callee).cloned();
                if let Some(generic_fn) = generic_fn {
                    let mut mono_fn = generic_fn.clone();
                    mono_fn.name = mono_name.clone();
                    // Offset the spans of this instantiation so the span-keyed
                    // semantic maps (`mono_calls`, `mono_structs`, `expr_types`,
                    // `decl_types`, ...) from different copies of the same
                    // generic function do not collide.
                    let offset = MONO_SPAN_BASE
                        + MONO_SPAN_COUNTER.fetch_add(1, Ordering::Relaxed) * MONO_SPAN_STRIDE;
                    nex_ast::offset_fn_decl(&mut mono_fn, offset);
                    for param in &mut mono_fn.params {
                        param.ty = substitute_type_expr(&param.ty, &mapping);
                    }
                    if let Some(ret) = &mut mono_fn.ret {
                        *ret = substitute_type_expr(ret, &mapping);
                    }
                    let mut mono_params = Vec::new();
                    for param in &mono_fn.params {
                        mono_params.push(self.resolve_type(&param.ty));
                    }
                    let mono_ret = if let Some(ret) = &mono_fn.ret {
                        self.resolve_type(ret)
                    } else {
                        Type::Void
                    };
                    self.analysis.functions.insert(
                        mono_name.clone(),
                        FnSig {
                            name: mono_name.clone(),
                            type_params: Vec::new(),
                            params: mono_params.clone(),
                            ret: mono_ret,
                            public: sig.public,
                            span: mono_fn.span,
                        },
                    );
                    self.analysis.mono_functions.push(mono_fn.clone());
                    self.check_one_function(&mono_fn);
                }
            }
            // now use mono_name as callee
            let mono_sig = self
                .analysis
                .functions
                .get(&mono_name)
                .cloned()
                .unwrap_or(sig.clone());
            // check args against mono_sig
            if args.len() != mono_sig.params.len() {
                self.error(
                    "NXS004",
                    format!("unknown function `{}`", callee),
                    callee_span,
                );
                for arg in args {
                    self.infer(arg);
                }
                return Type::Error;
            }
            for (arg, expected) in args.iter().zip(mono_sig.params.iter()) {
                self.check_expected(arg, *expected);
            }
            self.analysis.mono_calls.insert(span, mono_name.clone());
            return mono_sig.ret;
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
            for arg in args {
                self.infer(arg);
            }
            return Type::Error;
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
                "operands have different types `{left_ty}` and `{right_ty}`; NEX does not apply implicit widening"
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
        if ty.name == "&" || ty.name == "&mut" {
            if ty.generic_args.len() != 1 {
                self.error(
                    "NXS052",
                    "a reference type takes exactly one type argument".to_string(),
                    ty.span,
                );
                return Type::Error;
            }
            let inner = self.resolve_type(&ty.generic_args[0]);
            if inner == Type::Error {
                return Type::Error;
            }
            return Type::Ref {
                inner: inner.name(),
                mutable: ty.name == "&mut",
            };
        }
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
                        type_params: Vec::new(),
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
            match len {
                // `[T]`: a growable array, stored in the same registry with
                // `len: 0` as the "no fixed length" marker.
                None => {
                    let display = intern_dyn_array_type(elem_ty);
                    if !self.analysis.arrays.contains_key(display) {
                        self.analysis.arrays.insert(
                            display.to_string(),
                            ArrayInfo {
                                display,
                                llvm: DYN_ARRAY_LLVM,
                                elem: elem_ty,
                                len: 0,
                            },
                        );
                    }
                    return Type::DynArray(display);
                }
                Some(0) => {
                    self.error(
                        "NXS035",
                        "empty array types are not supported".to_string(),
                        ty.span,
                    );
                    return Type::Error;
                }
                Some(size) => {
                    let (display, llvm) = intern_array_type(elem_ty, *size);
                    if !self.analysis.arrays.contains_key(display) {
                        self.analysis.arrays.insert(
                            display.to_string(),
                            ArrayInfo {
                                display,
                                llvm,
                                elem: elem_ty,
                                len: *size,
                            },
                        );
                    }
                    return Type::Array { display, llvm };
                }
            }
        }

        if let Some(resolved) = Type::from_name(&ty.name) {
            if resolved == Type::Void {
                // `void` is only valid as a function return type. Callers can still
                // accept it; parameter/variable checks happen at the call site.
            }
            return resolved;
        }
        if self.current_type_params.contains(&ty.name) {
            let name = Box::leak(ty.name.clone().into_boxed_str());
            return Type::TypeParam(name);
        }
        if let Some(ty) = self.analysis.type_aliases.get(&ty.name).copied() {
            return ty;
        }
        if !ty.generic_args.is_empty() {
            if let Some(base_info) = self.analysis.structs.get(&ty.name).cloned() {
                if !base_info.type_params.is_empty()
                    && base_info.type_params.len() == ty.generic_args.len()
                {
                    let mut arg_types = Vec::new();
                    for arg in &ty.generic_args {
                        let resolved = self.resolve_type(arg);
                        if resolved == Type::Error {
                            return Type::Error;
                        }
                        arg_types.push(resolved);
                    }
                    let mut mapping = std::collections::HashMap::new();
                    for (param, concrete) in base_info.type_params.iter().zip(arg_types.iter()) {
                        mapping.insert(param.clone(), *concrete);
                    }
                    let mut concrete_names = Vec::new();
                    for param in &base_info.type_params {
                        let concrete = mapping.get(param).copied().unwrap_or(Type::Error);
                        concrete_names.push(concrete.name().to_string());
                    }
                    let mono_name = format!("{}_{}", ty.name, concrete_names.join("_"));
                    self.instantiate_generic_struct(&ty.name, &concrete_names, &mapping);
                    if let Some(mono_info) = self.analysis.structs.get(&mono_name) {
                        return Type::Struct(mono_info.name_static);
                    }
                }
            }
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
        // Globals live outside every scope: functions and spawned threads share
        // them, which is why `mutable` matters here.
        self.analysis.globals.get(name).map(|info| VarInfo {
            ty: info.ty,
            mutable: info.mutable,
        })
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
/// LLVM spelling of a growable array: three words (data, len, cap).
pub const DYN_ARRAY_LLVM: &str = "{ ptr, i64, i64 }";

/// Interns the display name of a growable array, e.g. `[i64]`.
fn intern_dyn_array_type(elem: Type) -> &'static str {
    Box::leak(format!("[{elem}]").into_boxed_str())
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
