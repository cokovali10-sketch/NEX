mod doctor;
mod project;

use nex_diagnostics::render_all;
use nex_driver::{CompileOptions, Emit, Profile, compile_file, compile_source};
use project::{create, discover, init};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const VERSION: &str = "0.1.0";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return ExitCode::SUCCESS;
    }

    match args[0].as_str() {
        "new" => cmd_new(&args[1..]),
        "init" => cmd_init(&args[1..]),
        "build" => cmd_build(&args[1..]),
        "run" => cmd_run(&args[1..]),
        "check" => cmd_check(&args[1..]),
        "test" => cmd_test(&args[1..]),
        "clean" => cmd_clean(&args[1..]),
        "doctor" => {
            let root = args.get(1).map(PathBuf::from);
            let project_root = root
                .as_deref()
                .and_then(|path| project::find_root(path).ok());
            doctor::run(project_root.as_deref());
            ExitCode::SUCCESS
        }
        "version" | "--version" | "-V" => {
            println!("nex {VERSION}");
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        "fmt" | "lint" | "add" | "benchmark" | "debug" | "profile" => {
            eprintln!(
                "nex: `{}` is planned for a later NEX release; see ROADMAP.md",
                args[0]
            );
            ExitCode::from(2)
        }
        _ => {
            eprintln!("nex: unknown command `{}`", args[0]);
            eprintln!("run `nex help` for usage");
            ExitCode::from(2)
        }
    }
}

fn cmd_new(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("nex: `new` requires a project name");
        return ExitCode::from(2);
    }
    let name = &args[0];
    if !valid_project_name(name) {
        eprintln!("nex: invalid project name `{name}`");
        return ExitCode::from(2);
    }
    let root = PathBuf::from(name);
    match create(&root, name) {
        Ok(()) => {
            println!("created project `{name}` in {}", root.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("nex: {error}");
            ExitCode::from(1)
        }
    }
}

fn cmd_init(args: &[String]) -> ExitCode {
    let root = if args.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&args[0])
    };
    match init(&root) {
        Ok(created) => {
            if created.is_empty() {
                println!("project already initialized");
            } else {
                for file in created {
                    println!("created {file}");
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("nex: {error}");
            ExitCode::from(1)
        }
    }
}

fn cmd_build(args: &[String]) -> ExitCode {
    let (path, emit, profile, output) = match parse_build_args(args, false) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(2);
        }
    };

    let project = match discover(Path::new(&path)) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(1);
        }
    };

    let options = CompileOptions {
        emit,
        profile,
        output,
        build_dir: Some(project.root.join("build")),
    };

    match compile_file(&project.entry, options) {
        Ok(result) => {
            if result.has_errors() {
                eprintln!("{}", render_all(&result.source, &result.diagnostics));
                ExitCode::from(1)
            } else {
                if let Some(artifacts) = result.artifacts {
                    if let Some(path) = artifacts.executable {
                        println!("built: {}", path.display());
                    } else if let Some(path) = artifacts.object {
                        println!("built: {}", path.display());
                    } else {
                        println!("built: {}", artifacts.llvm_ir.display());
                    }
                }
                let manifest_dir = project.root.join("build").join(match profile {
                    Profile::Debug => "debug",
                    Profile::Release => "release",
                    Profile::Max => "max",
                });
                let _ = fs::create_dir_all(&manifest_dir);
                let manifest_path = manifest_dir.join("build.json");
                let manifest = format!(
                    "{{\"version\":\"0.2.0\",\"profile\":\"{:?}\",\"entry\":\"{}\",\"target\":\"windows-x86_64\"}}\n",
                    profile,
                    project.entry.display()
                );
                let _ = fs::write(&manifest_path, manifest);
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("nex: {error}");
            ExitCode::from(2)
        }
    }
}

fn cmd_check(args: &[String]) -> ExitCode {
    let (path, _emit, _profile, _output) = match parse_build_args(args, false) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(2);
        }
    };

    let project = match discover(Path::new(&path)) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(1);
        }
    };

    let options = CompileOptions {
        emit: Emit::Check,
        profile: Profile::Debug,
        output: None,
        build_dir: Some(project.root.join("build")),
    };

    match compile_file(&project.entry, options) {
        Ok(result) => {
            if result.has_errors() {
                eprintln!("{}", render_all(&result.source, &result.diagnostics));
                ExitCode::from(1)
            } else {
                println!("check: OK");
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("nex: {error}");
            ExitCode::from(2)
        }
    }
}

fn cmd_run(args: &[String]) -> ExitCode {
    let split = args.iter().position(|arg| arg == "--");
    let (build_args, run_args) = if let Some(index) = split {
        (&args[..index], &args[index + 1..])
    } else {
        (args, &[][..])
    };

    let (path, _emit, profile, _output) = match parse_build_args(build_args, false) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(2);
        }
    };

    let project = match discover(Path::new(&path)) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(1);
        }
    };

    let options = CompileOptions {
        emit: Emit::Executable,
        profile,
        output: None,
        build_dir: Some(project.root.join("build")),
    };

    let executable = match compile_file(&project.entry, options) {
        Ok(result) => {
            if result.has_errors() {
                eprintln!("{}", render_all(&result.source, &result.diagnostics));
                return ExitCode::from(1);
            }
            result.artifacts.and_then(|artifacts| artifacts.executable)
        }
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(2);
        }
    };

    let Some(executable) = executable else {
        eprintln!("nex: no executable was produced");
        return ExitCode::from(1);
    };

    match Command::new(&executable).args(run_args).status() {
        Ok(status) => {
            if status.success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(status.code().unwrap_or(1) as u8)
            }
        }
        Err(error) => {
            eprintln!("nex: failed to run {}: {error}", executable.display());
            ExitCode::from(1)
        }
    }
}

fn cmd_clean(args: &[String]) -> ExitCode {
    let path = if args.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&args[0])
    };
    let project = match discover(&path) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(1);
        }
    };
    match project::clean(&project.root) {
        Ok(()) => {
            println!("cleaned {}", project.root.join("build").display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("nex: {error}");
            ExitCode::from(1)
        }
    }
}

fn cmd_test(args: &[String]) -> ExitCode {
    let path = if args.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&args[0])
    };
    let project = match discover(&path) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(1);
        }
    };

    let test_dir = project.root.join("tests");
    if !test_dir.is_dir() {
        println!("no tests directory");
        return ExitCode::SUCCESS;
    }

    let mut test_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&test_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("nex") {
                test_files.push(path);
            }
        }
    }
    test_files.sort();

    if test_files.is_empty() {
        println!("no .nex tests found");
        return ExitCode::SUCCESS;
    }

    let mut failed = 0usize;
    for test_file in &test_files {
        let source = match fs::read_to_string(test_file) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{}: {error}", test_file.display());
                failed += 1;
                continue;
            }
        };
        let stem = test_file
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("test");
        let build_dir = project.root.join("build").join("test").join(stem);
        let result = compile_source(
            test_file.display().to_string(),
            source,
            CompileOptions {
                emit: Emit::Executable,
                profile: Profile::Debug,
                output: None,
                build_dir: Some(build_dir),
            },
        );

        match result {
            Ok(result) if !result.has_errors() => {
                let executable = result.artifacts.and_then(|artifacts| artifacts.executable);
                match executable {
                    Some(executable) => match Command::new(&executable).status() {
                        Ok(status) if status.success() => {
                            println!("PASS {}", test_file.display());
                        }
                        Ok(status) => {
                            println!(
                                "FAIL {} (exit {})",
                                test_file.display(),
                                status.code().unwrap_or(-1)
                            );
                            failed += 1;
                        }
                        Err(error) => {
                            println!("FAIL {} ({error})", test_file.display());
                            failed += 1;
                        }
                    },
                    None => {
                        println!("FAIL {} (no executable)", test_file.display());
                        failed += 1;
                    }
                }
            }
            Ok(result) => {
                println!("FAIL {} (compilation errors)", test_file.display());
                eprintln!("{}", render_all(&result.source, &result.diagnostics));
                failed += 1;
            }
            Err(error) => {
                println!("FAIL {} ({error})", test_file.display());
                failed += 1;
            }
        }
    }

    if failed == 0 {
        println!("all {} tests passed", test_files.len());
        ExitCode::SUCCESS
    } else {
        eprintln!("{failed} of {} tests failed", test_files.len());
        ExitCode::from(1)
    }
}

fn parse_build_args(
    args: &[String],
    _allow_emit: bool,
) -> Result<(String, Emit, Profile, Option<PathBuf>), String> {
    let mut path = String::from(".");
    let mut emit = Emit::Executable;
    let mut profile = Profile::Debug;
    let mut output = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
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
            "--emit" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "`--emit` requires a value".to_string())?;
                emit = match value.as_str() {
                    "exe" | "executable" => Emit::Executable,
                    "obj" | "object" => Emit::Object,
                    "llvm-ir" | "ir" => Emit::LlvmIr,
                    "check" => Emit::Check,
                    _ => return Err(format!("unknown emit target `{value}`")),
                };
                index += 2;
            }
            "-o" | "--output" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "`-o` requires a path".to_string())?;
                output = Some(PathBuf::from(value));
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option `{value}`"));
            }
            _ => {
                path = args[index].clone();
                index += 1;
            }
        }
    }

    Ok((path, emit, profile, output))
}

fn valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        })
}

fn print_help() {
    println!(
        "nex {VERSION}\n\
         \n\
         Usage: nex <command> [options]\n\
         \n\
         Commands:\n\
           new <name>            Create a new NEX project\n\
           init [path]           Initialize a NEX project in an existing directory\n\
           build [path]          Build the current or specified project\n\
           run [path] [-- args]  Build and run the project\n\
           check [path]          Type-check without producing an executable\n\
           test [path]           Run .nex files in tests/\n\
           clean [path]          Remove build artifacts\n\
           doctor [path]         Check the toolchain and project\n\
           version               Print version information\n\
           help                  Show this help\n\
         \n\
         Build options:\n\
           --release             Release build\n\
           --max                 Maximum optimization build\n\
           --emit <kind>         exe | obj | llvm-ir | check\n\
           -o, --output <path>   Write the artifact to a specific path\n\
         \n\
         Planned but not yet implemented: fmt, lint, add, benchmark, debug, profile"
    );
}
