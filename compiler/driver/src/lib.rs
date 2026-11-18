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
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            emit: Emit::Executable,
            profile: Profile::Debug,
            output: None,
            build_dir: None,
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

pub fn compile_source(
    name: impl Into<String>,
    source_text: String,
    options: CompileOptions,
) -> Result<CompileResult, DriverError> {
    let source_name = name.into();
    let mut visited = std::collections::HashSet::new();
    let (mut program, sources, mut diagnostics) =
        load_program_text(&source_name, source_text, &mut visited)?;

    let source = sources
        .first()
        .cloned()
        .unwrap_or_else(|| SourceFile::new(source_name.clone(), String::new()));

    let analysis: Analysis = analyze(&program);
    program.functions.retain(|f| !analysis.generic_functions.contains(&f.name));
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
        .compile(&llvm_ir, &build_dir, &stem, options.profile, options.emit)
        .map_err(DriverError::Toolchain)?;
    result.artifacts = Some(artifacts);

    Ok(result)
}

fn load_program_text(
    name: &str,
    text: String,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
) -> Result<(nex_ast::Program, Vec<SourceFile>, Vec<Diagnostic>), DriverError> {
    let source = SourceFile::new(name.to_string(), text.clone());
    let lexed = Lexer::new(&text).lex();
    let mut diagnostics = lexed.diagnostics.clone();
    let parsed = parse(&lexed.tokens);
    diagnostics.extend(parsed.diagnostics.clone());

    let mut program = parsed.program;
    let mut sources = vec![source];

    // Resolve file-based imports.
    let mut retained_uses = Vec::new();
    for use_decl in program.uses.clone() {
        if let Some(file) = &use_decl.file {
            let base_dir = std::path::Path::new(name)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            let imported_path = base_dir.join(file);
            let (imported_program, imported_sources, imported_diagnostics) =
                load_program_file(&imported_path, visited)?;
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

fn load_program_file(
    path: &std::path::Path,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
) -> Result<(nex_ast::Program, Vec<SourceFile>, Vec<Diagnostic>), DriverError> {
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical.clone()) {
        return Err(DriverError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cyclic file import: {}", canonical.display()),
        )));
    }
    let text = fs::read_to_string(&canonical).map_err(DriverError::Io)?;
    let name = canonical.display().to_string();
    let (program, sources, diagnostics) = load_program_text(&name, text, visited)?;
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
        std::fs::write(dir.join("src").join("math.nex"), "pub fn add(a: i64, b: i64) -> i64:\n    return a + b\n").unwrap();
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
}
