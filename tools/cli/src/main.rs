mod doctor;
mod project;

use nex_diagnostics::render_all;
use nex_driver::{CompileOptions, Emit, Profile, compile_file, compile_source};
use project::{create, discover, init};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
            let mut root: Option<PathBuf> = None;
            let mut target: Option<String> = None;
            let mut index = 1;
            while index < args.len() {
                match args[index].as_str() {
                    "--target" => {
                        if index + 1 >= args.len() {
                            eprintln!("nex: `--target` requires a value");
                            return ExitCode::from(2);
                        }
                        target = Some(args[index + 1].clone());
                        index += 2;
                    }
                    value => {
                        root = Some(PathBuf::from(value));
                        index += 1;
                    }
                }
            }
            let project_root = root
                .as_deref()
                .and_then(|path| project::find_root(path).ok());
            doctor::run(project_root.as_deref(), target.as_deref());
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
        "fmt" => cmd_fmt(&args[1..]),
        "lint" => cmd_lint(&args[1..]),
        "add" => cmd_add(&args[1..]),
        "benchmark" => cmd_benchmark(&args[1..]),
        "debug" => cmd_debug(&args[1..]),
        "profile" => cmd_profile(&args[1..]),
        _ => {
            eprintln!("nex: unknown command `{}`", args[0]);
            eprintln!("run `nex help` for usage");
            ExitCode::from(2)
        }
    }
}

fn cmd_lint(args: &[String]) -> ExitCode {
    let mut deny = false;
    let mut paths: Vec<String> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--deny-warnings" | "-D" => deny = true,
            "--help" | "-h" => {
                println!("usage: nex lint [--deny-warnings] <file.nex>...");
                println!();
                println!("  --deny-warnings   exit 1 when a warning is reported");
                return ExitCode::SUCCESS;
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        eprintln!("nex: `lint` requires at least one .nex file");
        return ExitCode::from(2);
    }

    let mut status = ExitCode::SUCCESS;
    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("nex: cannot read `{path}`: {error}");
                status = ExitCode::from(2);
                continue;
            }
        };

        let result = nex_lint::lint_source(&source);
        let file = nex_diagnostics::SourceFile::new(path.clone(), source.clone());

        if !result.diagnostics.is_empty() {
            eprintln!("nex: `{path}` does not compile; linting skipped");
            eprintln!("{}", render_all(&file, &result.diagnostics));
            status = ExitCode::from(1);
            continue;
        }

        if result.warnings.is_empty() {
            println!("{path}: no findings");
            continue;
        }

        for warning in &result.warnings {
            let (line, column) = file.line_col(warning.span.start);
            println!(
                "{path}:{line}:{column}: {}: {}",
                warning.code, warning.message
            );
        }
        println!("{} warning(s) in {path}", result.warnings.len());
        if deny {
            status = ExitCode::from(1);
        }
    }

    status
}
fn cmd_fmt(args: &[String]) -> ExitCode {
    let mut write = false;
    let mut check = false;
    let mut paths: Vec<String> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--write" | "-w" => write = true,
            "--check" => check = true,
            "--help" | "-h" => {
                println!("usage: nex fmt [--check | --write] <file.nex>...");
                println!();
                println!("  --check   exit 1 when a file is not formatted");
                println!("  --write   rewrite the file in place");
                println!();
                println!("With no flag the formatted source is printed to stdout.");
                println!("Comments are dropped: NEX comments are not preserved yet.");
                return ExitCode::SUCCESS;
            }
            other => paths.push(other.to_string()),
        }
    }

    if paths.is_empty() {
        eprintln!("nex: `fmt` requires at least one .nex file");
        return ExitCode::from(2);
    }

    let mut status = ExitCode::SUCCESS;
    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("nex: cannot read `{path}`: {error}");
                status = ExitCode::from(2);
                continue;
            }
        };

        let result = nex_fmt::format_source(&source);
        if result.has_errors() {
            eprintln!("nex: `{path}` is not valid NEX; it was left untouched");
            for _diagnostic in &result.diagnostics {
                let file = nex_diagnostics::SourceFile::new(path.clone(), source.clone());
                eprintln!("{}", render_all(&file, &result.diagnostics));
            }
            status = ExitCode::from(1);
            continue;
        }

        if let Some(reason) = &result.unsupported {
            eprintln!("nex: `{path}` was left untouched: {reason}");
            status = ExitCode::from(1);
            continue;
        }

        if result.had_comments {
            eprintln!("nex: warning: comments in `{path}` are dropped by `fmt`");
        }

        let changed = result.source != source;
        if check {
            if changed {
                println!("{path}: would reformat");
                status = ExitCode::from(1);
            } else {
                println!("{path}: already formatted");
            }
        } else if write {
            if changed {
                if let Err(error) = fs::write(&path, &result.source) {
                    eprintln!("nex: cannot write `{path}`: {error}");
                    status = ExitCode::from(2);
                }
            }
        } else {
            print!("{}", result.source);
        }
    }

    status
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
    let (path, emit, profile, output, target) = match parse_build_args(args, false) {
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
        target,
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
                let target = if cfg!(target_os = "windows") {
                    "windows-x86_64"
                } else if cfg!(target_os = "linux") {
                    "linux-x86_64"
                } else if cfg!(target_os = "macos") {
                    "macos-aarch64"
                } else {
                    "unknown"
                };
                let manifest = format!(
                    "{{\"version\":\"{}\",\"profile\":\"{:?}\",\"entry\":\"{}\",\"target\":\"{}\"}}\n",
                    env!("CARGO_PKG_VERSION"),
                    profile,
                    project.entry.display(),
                    target
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
    let (path, _emit, _profile, _output, _target) = match parse_build_args(args, false) {
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
        target: None,
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

    let (path, _emit, profile, _output, target) = match parse_build_args(build_args, false) {
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
        target,
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

fn cmd_add(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: nex add <path>");
        return ExitCode::from(2);
    }
    let source = PathBuf::from(&args[0]);
    if !source.is_dir() {
        eprintln!("nex: `{}` is not a directory", source.display());
        return ExitCode::from(2);
    }
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("dependency")
        .to_string();

    let project = match discover(&PathBuf::from(".")) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(1);
        }
    };

    let deps_dir = project.root.join("deps");
    if let Err(error) = std::fs::create_dir_all(&deps_dir) {
        eprintln!("nex: cannot create deps dir: {error}");
        return ExitCode::from(1);
    }
    let dest = deps_dir.join(&name);
    if dest.exists() {
        eprintln!("nex: dependency `{name}` already exists");
        return ExitCode::from(1);
    }
    if let Err(error) = copy_dir_all(&source, &dest) {
        eprintln!("nex: failed to copy dependency: {error}");
        return ExitCode::from(1);
    }

    let config_path = project.root.join("nex.toml");
    if config_path.is_file() {
        let mut text = std::fs::read_to_string(&config_path).unwrap_or_default();
        if !text.contains("[dependencies]") {
            text.push_str("\n[dependencies]\n");
        }
        if !text.contains(&format!("\"{name}\"")) {
            if let Some(idx) = text.find("[dependencies]") {
                let insert_pos = text[idx..]
                    .find('\n')
                    .map(|p| idx + p + 1)
                    .unwrap_or(text.len());
                text.insert_str(insert_pos, &format!("{name} = \"{}\"\n", source.display()));
            }
        }
        let _ = std::fs::write(&config_path, text);
    }

    println!("added dependency `{name}`");
    ExitCode::SUCCESS
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn cmd_benchmark(args: &[String]) -> ExitCode {
    let mut iterations: u32 = 10;
    let mut path = PathBuf::from(".");
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--iterations" | "-n" => {
                if i + 1 >= args.len() {
                    eprintln!("nex: `--iterations` requires a value");
                    return ExitCode::from(2);
                }
                iterations = args[i + 1].parse().unwrap_or(10);
                i += 2;
                continue;
            }
            other => path = PathBuf::from(other),
        }
        i += 1;
    }

    let project = match discover(&path) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("nex: {error}");
            return ExitCode::from(1);
        }
    };

    let options = CompileOptions {
        emit: Emit::Executable,
        profile: Profile::Release,
        output: None,
        build_dir: Some(project.root.join("build")),
        target: None,
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

    let mut total = std::time::Duration::ZERO;
    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let status = Command::new(&executable).output();
        total += start.elapsed();
        match status {
            Ok(output) => {
                if !output.status.success() {
                    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                    return ExitCode::from(output.status.code().unwrap_or(1) as u8);
                }
            }
            Err(error) => {
                eprintln!("nex: failed to run {}: {error}", executable.display());
                return ExitCode::from(1);
            }
        }
    }
    let avg = total / iterations.max(1);
    println!(
        "benchmark: {} iterations, average {:.3} ms",
        iterations,
        avg.as_secs_f64() * 1000.0
    );
    ExitCode::SUCCESS
}

fn cmd_debug(args: &[String]) -> ExitCode {
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
    let options = CompileOptions {
        emit: Emit::Executable,
        profile: Profile::Debug,
        output: None,
        build_dir: Some(project.root.join("build")),
        target: None,
    };
    let executable = match compile_file(&project.entry, options) {
        Ok(result) => {
            if result.has_errors() {
                eprintln!("{}", render_all(&result.source, &result.diagnostics));
                return ExitCode::from(1);
            }
            result.artifacts.and_then(|a| a.executable)
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
    match Command::new(&executable).status() {
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

fn cmd_profile(args: &[String]) -> ExitCode {
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
    let options = CompileOptions {
        emit: Emit::Executable,
        profile: Profile::Release,
        output: None,
        build_dir: Some(project.root.join("build")),
        target: None,
    };
    let executable = match compile_file(&project.entry, options) {
        Ok(result) => {
            if result.has_errors() {
                eprintln!("{}", render_all(&result.source, &result.diagnostics));
                return ExitCode::from(1);
            }
            result.artifacts.and_then(|a| a.executable)
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
    let start = std::time::Instant::now();
    let status = Command::new(&executable).status();
    let elapsed = start.elapsed();
    match status {
        Ok(s) => {
            println!("profile: {:.3} ms", elapsed.as_secs_f64() * 1000.0);
            if s.success() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(s.code().unwrap_or(1) as u8)
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
                target: None,
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

type BuildArgs = (String, Emit, Profile, Option<PathBuf>, Option<String>);

fn parse_build_args(args: &[String], _allow_emit: bool) -> Result<BuildArgs, String> {
    let mut path = String::from(".");
    let mut emit = Emit::Executable;
    let mut profile = Profile::Debug;
    let mut output = None;
    let mut target = None;
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
            "--target" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "`--target` requires a target triple".to_string())?;
                target = Some(value.clone());
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

    Ok((path, emit, profile, output, target))
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
           --target <triple>     Compile for a specific target triple
          -o, --output <path>   Write the artifact to a specific path\n\
         \n\
         Planned but not yet implemented: (none)"
    );
}
