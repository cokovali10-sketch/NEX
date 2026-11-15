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
    let source = SourceFile::new(source_name.clone(), source_text.clone());

    let mut diagnostics = Vec::new();

    let lexed = Lexer::new(&source_text).lex();
    diagnostics.extend(lexed.diagnostics.clone());

    let parsed = parse(&lexed.tokens);
    diagnostics.extend(parsed.diagnostics.clone());

    let analysis: Analysis = analyze(&parsed.program);
    diagnostics.extend(analysis.diagnostics.clone());

    let (build_dir, stem) = resolve_output(&options, &source_name);

    let mut result = CompileResult {
        source_name,
        source,
        diagnostics,
        llvm_ir: None,
        artifacts: None,
    };

    if result.has_errors() {
        return Ok(result);
    }

    let ir = match nex_ir::build(&parsed.program, &analysis) {
        Ok(ir) => ir,
        Err(message) => {
            result
                .diagnostics
                .push(Diagnostic::error("NXC001", message, parsed.program.span));
            return Ok(result);
        }
    };

    let llvm_ir = match nex_backend::emit_module(&ir) {
        Ok(text) => text,
        Err(message) => {
            result
                .diagnostics
                .push(Diagnostic::error("NXC002", message, parsed.program.span));
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
