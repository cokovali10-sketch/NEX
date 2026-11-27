pub use nex_backend::{BuildArtifacts, Clang, Emit, Profile, ToolchainError};
use nex_diagnostics::{Diagnostic, SourceFile};
use nex_lexer::Lexer;
use nex_parser::parse;
use nex_semantic::{Analysis, analyze};
use std::fs;
use std::path::{Path, PathBuf};

pub use nex_backend::{
    BuildArtifacts as NexBuildArtifacts, Clang as NexClang, Emit as NexEmit, Profile as NexProfile,
};

#[derive(Debug, Clone)]
pub struct CompileOptions {
    pub emit: Emit,
    pub profile: Profile,
    pub output: Option<PathBuf>,
    pub build_dir: Option<PathBuf>,
    pub target: Option<String>,
    pub sanitize: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            emit: Emit::Executable,
            profile: Profile::Debug,
            output: None,
            build_dir: None,
            target: None,
            sanitize: false,
        }
    }
}

#[derive(Debug)]
pub enum DriverError {
    Io(std::io::Error),
    Toolchain(ToolchainError),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Toolchain(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for DriverError {}

#[derive(Debug)]
pub struct CompileResult {
    pub source_name: String,
    pub source: SourceFile,
    pub sources: Vec<SourceFile>,
    pub diagnostics: Vec<Diagnostic>,
    pub llvm_ir: Option<String>,
    pub artifacts: Option<BuildArtifacts>,
}

impl CompileResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }
}

/// Stack size for compiler work. Recursive parser/checker/IR passes consume
/// deep stacks on deeply nested input; a large fixed stack lets the parser's
/// nesting limit report a diagnostic instead of aborting the process.
pub const COMPILER_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Runs `task` on a worker thread with a large stack and propagates panics.
pub fn run_with_large_stack<F, T>(task: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let handle = std::thread::Builder::new()
        .name("nex-compiler".to_string())
        .stack_size(COMPILER_STACK_SIZE)
        .spawn(task)
        .expect("failed to spawn the NEX compiler thread");
    match handle.join() {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub fn compile_source(
    name: impl Into<String>,
    source_text: String,
    options: CompileOptions,
) -> Result<CompileResult, DriverError> {
    let name = name.into();
    run_with_large_stack(move || compile_source_inner(name, source_text, options))
}

fn compile_source_inner(
    source_name: String,
    source_text: String,
    options: CompileOptions,
) -> Result<CompileResult, DriverError> {
    let mut visited = std::collections::HashSet::new();
    let mut next_span_offset = 0usize;
    let (mut program, sources, mut diagnostics) = load_program_text(
        &source_name,
        source_text,
        &mut visited,
        &mut next_span_offset,
    )?;

    let source = sources
        .first()
        .cloned()
        .unwrap_or_else(|| SourceFile::new(source_name.clone(), String::new()));

    let analysis: Analysis = analyze(&program);
    program
        .functions
        .retain(|f| !analysis.generic_functions.contains(&f.name));
    diagnostics.extend(analysis.diagnostics.clone());

    let (build_dir, stem) = resolve_output(&options, &source_name);

    let mut result = CompileResult {
        source_name,
        source,
        sources,
        diagnostics,
        llvm_ir: None,
        artifacts: None,
    };

    if result.has_errors() {
        return Ok(result);
    }

    let ir = match nex_ir::build(&program, &analysis) {
        Ok(ir) => ir,
        Err(message) => {
            result
                .diagnostics
                .push(Diagnostic::error("NXC001", message, program.span));
            return Ok(result);
        }
    };

    let llvm_ir = match nex_backend::emit_module(&ir) {
        Ok(text) => text,
        Err(message) => {
            result
                .diagnostics
                .push(Diagnostic::error("NXC002", message, program.span));
            return Ok(result);
        }
    };
    result.llvm_ir = Some(llvm_ir.clone());

    if matches!(options.emit, Emit::Check) {
        return Ok(result);
    }

    let clang = Clang::discover().map_err(DriverError::Toolchain)?;
    let artifacts = clang
        .compile(
            &llvm_ir,
            &build_dir,
            &stem,
            options.profile,
            options.emit,
            options.target.as_deref(),
            options.sanitize,
        )
        .map_err(DriverError::Toolchain)?;
    result.artifacts = Some(artifacts);

    Ok(result)
}

fn load_program_text(
    name: &str,
    text: String,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    next_span_offset: &mut usize,
) -> Result<(nex_ast::Program, Vec<SourceFile>, Vec<Diagnostic>), DriverError> {
    let span_offset = *next_span_offset;
    *next_span_offset = next_span_offset.saturating_add(text.len().saturating_add(1));
    let source = SourceFile::new(name.to_string(), text.clone());
    let lexed = Lexer::new(&text).lex();
    let mut diagnostics = lexed.diagnostics.clone();
    let parsed = parse(&lexed.tokens);
    diagnostics.extend(parsed.diagnostics.clone());

    let mut program = parsed.program;
    nex_ast::offset_program(&mut program, span_offset);
    let mut sources = vec![source];

    // Resolve file-based imports.
    let mut retained_uses = Vec::new();
    for use_decl in program.uses.clone() {
        if let Some(file) = &use_decl.file {
            let base_dir = std::path::Path::new(name)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            let imported_path = resolve_import_path(base_dir, file)?;
            let (imported_program, imported_sources, imported_diagnostics) =
                load_program_file(&imported_path, visited, next_span_offset)?;
            diagnostics.extend(imported_diagnostics);

            let alias = if use_decl.path.len() == 1 {
                use_decl.path[0].clone()
            } else {
                use_decl.path.join(".")
            };
            let prefixed = prefix_program(imported_program, &alias);
            program.modules.push(alias);
            program.structs.extend(prefixed.structs);
            program.enums.extend(prefixed.enums);
            program.traits.extend(prefixed.traits);
            program.impls.extend(prefixed.impls);
            program.consts.extend(prefixed.consts);
            program.globals.extend(prefixed.globals);
            program.functions.extend(prefixed.functions);
            sources.extend(imported_sources);
        } else {
            retained_uses.push(use_decl);
        }
    }
    program.uses = retained_uses;

    Ok((program, sources, diagnostics))
}

fn resolve_import_path(base_dir: &Path, requested: &str) -> Result<PathBuf, DriverError> {
    let requested_path = Path::new(requested);
    if requested_path.is_absolute() {
        if requested_path.is_file() {
            return Ok(requested_path.to_path_buf());
        }
    } else {
        let direct = base_dir.join(requested_path);
        if direct.is_file() {
            return Ok(direct);
        }
        if requested_path.extension().is_none() {
            let with_extension = direct.with_extension("nex");
            if with_extension.is_file() {
                return Ok(with_extension);
            }
        }

        let project_root = base_dir
            .ancestors()
            .find(|ancestor| ancestor.join("nex.toml").is_file());
        let search_roots: Vec<&Path> = if let Some(root) = project_root {
            base_dir
                .ancestors()
                .take_while(|ancestor| *ancestor != root)
                .chain(std::iter::once(root))
                .collect()
        } else {
            base_dir.ancestors().collect()
        };

        for root in search_roots {
            let candidate = root.join("deps").join(requested_path);
            if candidate.is_file() {
                return Ok(candidate);
            }
            if requested_path.extension().is_none() {
                let with_extension = candidate.with_extension("nex");
                if with_extension.is_file() {
                    return Ok(with_extension);
                }
                if candidate.is_dir() {
                    for entry in [
                        candidate.join("lib.nex"),
                        candidate.join("src").join("lib.nex"),
                        candidate.join("main.nex"),
                        candidate.join("src").join("main.nex"),
                    ] {
                        if entry.is_file() {
                            return Ok(entry);
                        }
                    }
                }
            }
        }
    }

    Err(DriverError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("cannot resolve imported NEX module `{requested}`"),
    )))
}

fn load_program_file(
    path: &std::path::Path,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    next_span_offset: &mut usize,
) -> Result<(nex_ast::Program, Vec<SourceFile>, Vec<Diagnostic>), DriverError> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        return Err(DriverError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cyclic file import: {}", canonical.display()),
        )));
    }
    let text = fs::read_to_string(&canonical).map_err(DriverError::Io)?;
    let name = canonical.display().to_string();
    let (program, sources, diagnostics) =
        load_program_text(&name, text, visited, next_span_offset)?;
    // Tag diagnostics from imported files with their real file and position.
    let primary = sources.first();
    let tagged: Vec<Diagnostic> = diagnostics
        .into_iter()
        .map(|mut diagnostic| {
            let location = diagnostic
                .span
                .and_then(|span| primary.map(|source| source.line_col(span.start)))
                .map(|(line, column)| format!(":{line}:{column}"))
                .unwrap_or_default();
            diagnostic.message = format!("{name}{location}: {}", diagnostic.message);
            diagnostic.span = None;
            diagnostic
        })
        .collect();
    visited.remove(&canonical);
    Ok((program, sources, tagged))
}

fn prefix_program(mut program: nex_ast::Program, prefix: &str) -> nex_ast::Program {
    // Imported declarations are renamed to `module.name`. Keep calls between
    // functions declared in the same source file in sync before renaming them.
    // Without this, a library's public function could not call its own private
    // helpers after a file import.
    let local_functions: std::collections::HashSet<String> = program
        .functions
        .iter()
        .map(|function| function.name.clone())
        .collect();
    for global in &mut program.globals {
        prefix_local_calls(&mut global.init, &local_functions, prefix);
    }
    for constant in &mut program.consts {
        prefix_local_calls(&mut constant.init, &local_functions, prefix);
    }
    for function in &mut program.functions {
        prefix_local_calls_in_block(&mut function.body, &local_functions, prefix);
    }
    for implementation in &mut program.impls {
        for method in &mut implementation.methods {
            prefix_local_calls_in_block(&mut method.body, &local_functions, prefix);
        }
    }

    for item in &mut program.structs {
        item.name = format!("{prefix}.{}", item.name);
    }
    for item in &mut program.enums {
        item.name = format!("{prefix}.{}", item.name);
    }
    for item in &mut program.traits {
        item.name = format!("{prefix}.{}", item.name);
    }
    for item in &mut program.impls {
        item.trait_name = format!("{prefix}.{}", item.trait_name);
        item.type_name = format!("{prefix}.{}", item.type_name);
        for method in &mut item.methods {
            method.name = format!("{prefix}.{}", method.name);
        }
    }
    for item in &mut program.consts {
        item.name = format!("{prefix}.{}", item.name);
    }
    for item in &mut program.globals {
        item.name = format!("{prefix}.{}", item.name);
    }
    for item in &mut program.functions {
        item.name = format!("{prefix}.{}", item.name);
    }
    program.modules.push(prefix.to_string());
    program
}

fn prefix_local_calls_in_block(
    block: &mut nex_ast::Block,
    local_functions: &std::collections::HashSet<String>,
    prefix: &str,
) {
    use nex_ast::StmtKind;

    for statement in &mut block.stmts {
        match &mut statement.kind {
            StmtKind::Let { init, .. }
            | StmtKind::Assign { value: init, .. }
            | StmtKind::AssignField { value: init, .. } => {
                prefix_local_calls(init, local_functions, prefix);
            }
            StmtKind::AssignMany { values, .. } => {
                for value in values {
                    prefix_local_calls(value, local_functions, prefix);
                }
            }
            StmtKind::AssignIndex { index, value, .. }
            | StmtKind::AssignIndexDeref { index, value, .. } => {
                prefix_local_calls(index, local_functions, prefix);
                prefix_local_calls(value, local_functions, prefix);
            }
            StmtKind::AssignDeref { place, value } => {
                prefix_local_calls(place, local_functions, prefix);
                prefix_local_calls(value, local_functions, prefix);
            }
            StmtKind::AssignFieldDeref {
                reference, value, ..
            } => {
                prefix_local_calls(reference, local_functions, prefix);
                prefix_local_calls(value, local_functions, prefix);
            }
            StmtKind::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    prefix_local_calls(condition, local_functions, prefix);
                    prefix_local_calls_in_block(body, local_functions, prefix);
                }
                if let Some(body) = else_branch {
                    prefix_local_calls_in_block(body, local_functions, prefix);
                }
            }
            StmtKind::While { cond, body } => {
                prefix_local_calls(cond, local_functions, prefix);
                prefix_local_calls_in_block(body, local_functions, prefix);
            }
            StmtKind::For {
                start, end, body, ..
            } => {
                prefix_local_calls(start, local_functions, prefix);
                prefix_local_calls(end, local_functions, prefix);
                prefix_local_calls_in_block(body, local_functions, prefix);
            }
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    prefix_local_calls(value, local_functions, prefix);
                }
            }
            StmtKind::Match { scrutinee, arms } => {
                prefix_local_calls(scrutinee, local_functions, prefix);
                for arm in arms {
                    if let Some(guard) = &mut arm.guard {
                        prefix_local_calls(guard, local_functions, prefix);
                    }
                    prefix_local_calls_in_block(&mut arm.body, local_functions, prefix);
                }
            }
            StmtKind::Unsafe(body) | StmtKind::Defer(body) => {
                prefix_local_calls_in_block(body, local_functions, prefix);
            }
            StmtKind::Expr(expression) => {
                prefix_local_calls(expression, local_functions, prefix);
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
        }
    }
}

fn prefix_local_calls(
    expression: &mut nex_ast::Expr,
    local_functions: &std::collections::HashSet<String>,
    prefix: &str,
) {
    use nex_ast::ExprKind;

    match &mut expression.kind {
        ExprKind::Unary { operand, .. } | ExprKind::Try(operand) | ExprKind::Deref(operand) => {
            prefix_local_calls(operand, local_functions, prefix)
        }
        ExprKind::Binary { left, right, .. } => {
            prefix_local_calls(left, local_functions, prefix);
            prefix_local_calls(right, local_functions, prefix);
        }
        ExprKind::Call { callee, args, .. } => {
            if local_functions.contains(callee) {
                *callee = format!("{prefix}.{callee}");
            }
            for argument in args {
                prefix_local_calls(argument, local_functions, prefix);
            }
        }
        ExprKind::Cast { expr, .. } | ExprKind::Borrow { place: expr, .. } => {
            prefix_local_calls(expr, local_functions, prefix);
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                prefix_local_calls(item, local_functions, prefix);
            }
        }
        ExprKind::Index { base, index } => {
            prefix_local_calls(base, local_functions, prefix);
            prefix_local_calls(index, local_functions, prefix);
        }
        ExprKind::Field { base, .. } => prefix_local_calls(base, local_functions, prefix),
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Char(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Error => {}
    }
}

pub fn compile_file(path: &Path, options: CompileOptions) -> Result<CompileResult, DriverError> {
    let source_text = fs::read_to_string(path).map_err(DriverError::Io)?;
    compile_source(path.display().to_string(), source_text, options)
}

fn resolve_output(options: &CompileOptions, source_name: &str) -> (PathBuf, String) {
    if let Some(output) = &options.output {
        let build_dir = output
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = output
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "nex_program".to_string());
        return (build_dir, stem);
    }

    let build_dir = options
        .build_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("build"));
    let build_dir = build_dir.join(options.profile.name());

    let stem = Path::new(source_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "nex_program".to_string());

    (build_dir, stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_a_file_module() {
        let dir = std::env::temp_dir().join("nex-driver-import");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src").join("math.nex"),
            "pub fn add(a: i64, b: i64) -> i64:\n    return a + b\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src").join("main.nex"),
            "use math from \"math.nex\"\n\nfn main():\n    print(math.add(2, 3))\n",
        )
        .unwrap();
        let result = compile_file(
            &dir.join("src").join("main.nex"),
            CompileOptions {
                emit: Emit::Executable,
                build_dir: Some(dir.join("build")),
                ..CompileOptions::default()
            },
        )
        .expect("compilation should not fail at toolchain level");
        assert!(!result.has_errors(), "{:?}", result.diagnostics);
        assert!(result.artifacts.as_ref().unwrap().executable.is_some());
    }

    #[test]
    fn imported_functions_can_call_private_helpers() {
        let dir = std::env::temp_dir().join("nex-driver-import-helper");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src").join("math.nex"),
            "fn twice(value: i64) -> i64:\n    return value * 2\n\npub fn four_times(value: i64) -> i64:\n    return twice(twice(value))\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src").join("main.nex"),
            "use math from \"math.nex\"\n\nfn main():\n    print(math.four_times(3))\n",
        )
        .unwrap();

        let result = compile_file(
            &dir.join("src").join("main.nex"),
            CompileOptions {
                emit: Emit::Check,
                build_dir: Some(dir.join("build")),
                ..CompileOptions::default()
            },
        )
        .expect("checking an imported helper should succeed");
        assert!(!result.has_errors(), "{:?}", result.diagnostics);
    }

    #[test]
    fn resolves_short_import_names_from_project_dependencies() {
        let dir = std::env::temp_dir().join("nex-driver-dependency-import");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("deps")).unwrap();
        std::fs::write(dir.join("nex.toml"), "[package]\nname = \"test\"\n").unwrap();
        std::fs::write(
            dir.join("deps").join("textx.nex"),
            "pub fn twice(value: i64) -> i64:\n    return value * 2\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src").join("main.nex"),
            "use textx from \"textx\"\n\nfn main():\n    print(textx.twice(3))\n",
        )
        .unwrap();

        let result = compile_file(
            &dir.join("src").join("main.nex"),
            CompileOptions {
                emit: Emit::Check,
                build_dir: Some(dir.join("build")),
                ..CompileOptions::default()
            },
        )
        .expect("dependency import should resolve");
        assert!(!result.has_errors(), "{:?}", result.diagnostics);
    }

    #[test]
    fn compiles_hello_world() {
        let result = compile_source(
            "hello.nex",
            "fn main():\n    print(\"Hello from NEX!\")\n".to_string(),
            CompileOptions {
                emit: Emit::Executable,
                build_dir: Some(std::env::temp_dir().join("nex-driver-hello")),
                ..CompileOptions::default()
            },
        )
        .expect("compilation should not fail at toolchain level");
        assert!(!result.has_errors(), "{:?}", result.diagnostics);
        assert!(result.artifacts.as_ref().unwrap().executable.is_some());
    }

    #[test]
    fn imports_with_keyword_syntax() {
        let dir = std::env::temp_dir().join("nex-driver-import-keyword");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src").join("math.nex"),
            "pub fn add(a: i64, b: i64) -> i64:\n    return a + b\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src").join("main.nex"),
            "import \"math.nex\"\n\nfn main():\n    print(math.add(2, 3))\n",
        )
        .unwrap();
        let result = compile_file(
            &dir.join("src").join("main.nex"),
            CompileOptions {
                emit: Emit::Executable,
                build_dir: Some(dir.join("build")),
                ..CompileOptions::default()
            },
        )
        .expect("compilation should not fail at toolchain level");
        assert!(!result.has_errors(), "{:?}", result.diagnostics);
        assert!(result.artifacts.as_ref().unwrap().executable.is_some());
    }
}
