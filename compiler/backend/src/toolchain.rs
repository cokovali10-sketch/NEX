use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUNTIME_C: &str = include_str!("../../../runtime/core/nex_rt.c");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Debug,
    Release,
    Max,
}

impl Profile {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
            Self::Max => "max",
        }
    }

    pub const fn clang_optimization(self) -> &'static str {
        match self {
            Self::Debug => "-O0",
            Self::Release => "-O2",
            Self::Max => "-O3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    Check,
    LlvmIr,
    Object,
    Executable,
}

#[derive(Debug, Clone)]
pub struct BuildArtifacts {
    pub llvm_ir: PathBuf,
    pub object: Option<PathBuf>,
    pub runtime_object: Option<PathBuf>,
    pub executable: Option<PathBuf>,
}

#[derive(Debug)]
pub enum ToolchainError {
    NotFound(String),
    CommandFailed { command: String, stderr: String },
    Io(std::io::Error),
}

impl std::fmt::Display for ToolchainError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message) => formatter.write_str(message),
            Self::CommandFailed { command, stderr } => {
                write!(formatter, "command `{command}` failed: {stderr}")
            }
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for ToolchainError {}

#[derive(Debug, Clone)]
pub struct Clang {
    pub path: PathBuf,
    pub version: String,
    pub target: String,
}

impl Clang {
    pub fn discover() -> Result<Self, ToolchainError> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        if let Some(path) = env::var_os("NEX_CLANG") {
            candidates.push(PathBuf::from(path));
        }

        candidates.push(PathBuf::from("clang"));

        if cfg!(windows) {
            candidates.extend(windows_candidates());
        } else {
            candidates.push(PathBuf::from("/usr/bin/clang"));
            candidates.push(PathBuf::from("/usr/local/bin/clang"));
        }

        for candidate in candidates {
            if let Ok(output) = Command::new(&candidate).arg("--version").output() {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let version = stdout.lines().next().unwrap_or("clang").to_string();
                    let target = Command::new(&candidate)
                        .arg("-dumpmachine")
                        .output()
                        .ok()
                        .and_then(|output| {
                            if output.status.success() {
                                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    return Ok(Self {
                        path: candidate,
                        version,
                        target,
                    });
                }
            }
        }

        Err(ToolchainError::NotFound(
            "could not find `clang`; install LLVM/Clang or set NEX_CLANG".to_string(),
        ))
    }

    pub fn probe(&self) -> Result<(), ToolchainError> {
        let temp = unique_temp_dir("nex-toolchain-probe");
        fs::create_dir_all(&temp).map_err(ToolchainError::Io)?;
        let source = temp.join("probe.c");
        let output = temp.join("probe.exe");
        fs::write(
            &source,
            "#include <stdio.h>\nint main(void){puts(\"nex-probe-ok\");return 0;}\n",
        )
        .map_err(ToolchainError::Io)?;
        self.run(
            &[source.to_string_lossy().to_string()],
            &output,
            Profile::Debug,
            false,
        )?;
        let _ = fs::remove_dir_all(&temp);
        Ok(())
    }

    pub fn compile(
        &self,
        ir_text: &str,
        build_dir: &Path,
        stem: &str,
        profile: Profile,
        emit: Emit,
    ) -> Result<BuildArtifacts, ToolchainError> {
        fs::create_dir_all(build_dir).map_err(ToolchainError::Io)?;
        let llvm_ir = build_dir.join(format!("{stem}.ll"));
        fs::write(&llvm_ir, ir_text).map_err(ToolchainError::Io)?;

        match emit {
            Emit::Check | Emit::LlvmIr => Ok(BuildArtifacts {
                llvm_ir,
                object: None,
                runtime_object: None,
                executable: None,
            }),
            Emit::Object => {
                let object = build_dir.join(object_name(stem));
                let runtime_object = build_dir.join("nex_rt.o");
                self.compile_ir_to_object(&llvm_ir, &object, profile)?;
                self.compile_runtime_to_object(&runtime_object, profile)?;
                Ok(BuildArtifacts {
                    llvm_ir,
                    object: Some(object),
                    runtime_object: Some(runtime_object),
                    executable: None,
                })
            }
            Emit::Executable => {
                let executable = build_dir.join(executable_name(stem));
                self.link_executable(&llvm_ir, &executable, profile)?;
                Ok(BuildArtifacts {
                    llvm_ir,
                    object: None,
                    runtime_object: None,
                    executable: Some(executable),
                })
            }
        }
    }

    fn compile_ir_to_object(
        &self,
        llvm_ir: &Path,
        output: &Path,
        profile: Profile,
    ) -> Result<(), ToolchainError> {
        let mut args = vec![
            profile.clang_optimization().to_string(),
            "-c".to_string(),
            llvm_ir.to_string_lossy().to_string(),
            "-o".to_string(),
            output.to_string_lossy().to_string(),
        ];
        if profile == Profile::Debug {
            args.push("-g".to_string());
        }
        self.run_with_args(&args)
    }

    fn compile_runtime_to_object(
        &self,
        output: &Path,
        profile: Profile,
    ) -> Result<(), ToolchainError> {
        let runtime_path =
            build_dir_runtime_source(output.parent().unwrap_or_else(|| Path::new(".")), RUNTIME_C)?;
        let mut args = vec![
            profile.clang_optimization().to_string(),
            "-c".to_string(),
            runtime_path.to_string_lossy().to_string(),
            "-o".to_string(),
            output.to_string_lossy().to_string(),
        ];
        if profile == Profile::Debug {
            args.push("-g".to_string());
        }
        self.run_with_args(&args)
    }

    fn link_executable(
        &self,
        llvm_ir: &Path,
        output: &Path,
        profile: Profile,
    ) -> Result<(), ToolchainError> {
        let runtime_path =
            build_dir_runtime_source(output.parent().unwrap_or_else(|| Path::new(".")), RUNTIME_C)?;
        let mut args = vec![
            profile.clang_optimization().to_string(),
            llvm_ir.to_string_lossy().to_string(),
            runtime_path.to_string_lossy().to_string(),
            "-o".to_string(),
            output.to_string_lossy().to_string(),
        ];
        if profile == Profile::Debug {
            args.push("-g".to_string());
        }
        if cfg!(windows) {
            args.push("-static".to_string());
            // The runtime's socket helpers live in ws2_32 on Windows.
            args.push("-lws2_32".to_string());
        } else {
            // The runtime uses sqrt/pow from libm; link it explicitly on Unix.
            args.push("-lm".to_string());
        }
        self.run_with_args(&args)
    }

    fn run(
        &self,
        inputs: &[String],
        output: &Path,
        profile: Profile,
        static_link: bool,
    ) -> Result<(), ToolchainError> {
        let mut args = vec![profile.clang_optimization().to_string()];
        args.extend(inputs.iter().cloned());
        args.push("-o".to_string());
        args.push(output.to_string_lossy().to_string());
        if static_link && cfg!(windows) {
            args.push("-static".to_string());
        }
        self.run_with_args(&args)
    }

    fn run_with_args(&self, args: &[String]) -> Result<(), ToolchainError> {
        let output = Command::new(&self.path)
            .args(args)
            .output()
            .map_err(ToolchainError::Io)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(ToolchainError::CommandFailed {
                command: format!("{} {}", self.path.display(), args.join(" ")),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            })
        }
    }
}

fn windows_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("ProgramFiles") {
        candidates.push(
            PathBuf::from(path)
                .join("LLVM")
                .join("bin")
                .join("clang.exe"),
        );
    }
    if let Some(path) = env::var_os("ProgramFiles(x86)") {
        candidates.push(
            PathBuf::from(path)
                .join("LLVM")
                .join("bin")
                .join("clang.exe"),
        );
    }
    let llvm_mingw_root = PathBuf::from("D:\\llvm-mingw");
    if llvm_mingw_root.exists() {
        if let Ok(entries) = fs::read_dir(&llvm_mingw_root) {
            for entry in entries.flatten() {
                let clang = entry.path().join("bin").join("clang.exe");
                if clang.exists() {
                    candidates.push(clang);
                }
            }
        }
    }
    candidates
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

fn build_dir_runtime_source(build_dir: &Path, source: &str) -> Result<PathBuf, ToolchainError> {
    fs::create_dir_all(build_dir).map_err(ToolchainError::Io)?;
    let path = build_dir.join("nex_rt.c");
    fs::write(&path, source).map_err(ToolchainError::Io)?;
    Ok(path)
}

fn object_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.obj")
    } else {
        format!("{stem}.o")
    }
}

fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

pub fn temp_dir(prefix: &str) -> PathBuf {
    unique_temp_dir(prefix)
}
