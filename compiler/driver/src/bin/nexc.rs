use nex_diagnostics::render_all;
use nex_driver::{CompileOptions, Emit, Profile, compile_file};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut emit = Emit::Executable;
    let mut profile = Profile::Debug;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--emit" => {
                let value = match args.get(index + 1) {
                    Some(value) => value.clone(),
                    None => {
                        eprintln!("nexc: `--emit` requires a value");
                        return ExitCode::from(2);
                    }
                };
                emit = match value.as_str() {
                    "exe" | "executable" => Emit::Executable,
                    "obj" | "object" => Emit::Object,
                    "llvm-ir" | "ir" => Emit::LlvmIr,
                    "check" => Emit::Check,
                    _ => {
                        eprintln!("nexc: unknown emit target `{value}`");
                        return ExitCode::from(2);
                    }
                };
                index += 2;
            }
            "--output" | "-o" => {
                let value = match args.get(index + 1) {
                    Some(value) => value.clone(),
                    None => {
                        eprintln!("nexc: `-o` requires a path");
                        return ExitCode::from(2);
                    }
                };
                output = Some(PathBuf::from(value));
                index += 2;
            }
            "--release" => {
                profile = Profile::Release;
                index += 1;
            }
            "--max" => {
                profile = Profile::Max;
                index += 1;
            }
            "--debug" => {
                profile = Profile::Debug;
                index += 1;
            }
            value if value.starts_with('-') => {
                eprintln!("nexc: unknown option `{value}`");
                return ExitCode::from(2);
            }
            _ => {
                if input.is_some() {
                    eprintln!("nexc: only one input file is accepted");
                    return ExitCode::from(2);
                }
                input = Some(PathBuf::from(&args[index]));
                index += 1;
            }
        }
    }

    let Some(input) = input else {
        eprintln!("nexc: missing input file");
        return ExitCode::from(2);
    };

    let options = CompileOptions {
        emit,
        profile,
        output,
        build_dir: None,
    };

    match compile_file(&input, options) {
        Ok(result) => {
            if result.has_errors() {
                eprintln!("{}", render_all(&result.source, &result.diagnostics));
                ExitCode::from(1)
            } else {
                if let Some(artifacts) = &result.artifacts {
                    if let Some(path) = &artifacts.executable {
                        println!("compiled: {}", path.display());
                    } else if let Some(path) = &artifacts.object {
                        println!("compiled object: {}", path.display());
                    } else {
                        println!("compiled: {}", artifacts.llvm_ir.display());
                    }
                } else if let Some(ir) = &result.llvm_ir {
                    let path = std::env::current_dir().unwrap_or_default().join("nex.ll");
                    let _ = std::fs::write(&path, ir);
                    println!("wrote LLVM IR: {}", path.display());
                }
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("nexc: {error}");
            ExitCode::from(2)
        }
    }
}

fn print_help() {
    println!(
        "nexc 0.1.0\n\
         \n\
         Usage: nexc [options] <file.nex>\n\
         \n\
         Options:\n\
           -o, --output <path>   Write the requested artifact to this path\n\
           --emit <kind>         exe | obj | llvm-ir | check\n\
           --debug               Debug build (default)\n\
           --release             Release build\n\
           --max                 Maximum optimization build\n\
           -h, --help            Show this help"
    );
}
