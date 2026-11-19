use nex_backend::Clang;
use nex_driver::{CompileOptions, Emit, Profile, compile_source};
use std::path::Path;

pub fn run(project_root: Option<&Path>, target: Option<&str>) {
    println!("NEX doctor");
    println!("==========");
    println!("os: {}", std::env::consts::OS);
    println!("arch: {}", std::env::consts::ARCH);
    println!("compiler: NEX {}", env!("CARGO_PKG_VERSION"));

    match Clang::discover() {
        Ok(clang) => {
            println!("clang: OK");
            println!("  path: {}", clang.path.display());
            println!("  version: {}", clang.version);
            println!("  target: {}", clang.target);
            match clang.probe() {
                Ok(()) => println!("  probe: OK"),
                Err(error) => println!("  probe: FAIL ({error})"),
            }
            if let Some(triple) = target {
                match clang.validate_target(triple) {
                    Ok(()) => println!("  target {triple}: OK"),
                    Err(error) => println!("  target {triple}: FAIL ({error})"),
                }
            }
        }
        Err(error) => {
            println!("clang: FAIL ({error})");
        }
    }

    let temp = nex_backend::temp_dir("nex-doctor");
    let _ = std::fs::create_dir_all(&temp);
    let build_dir = temp.join("build").join("debug");
    let result = compile_source(
        "doctor.nex",
        "fn main():\n    print(\"doctor-ok\")\n".to_string(),
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
            if let Some(artifacts) = result.artifacts {
                if let Some(executable) = artifacts.executable {
                    match std::process::Command::new(&executable).output() {
                        Ok(output) if output.status.success() => {
                            println!("end-to-end compile: OK");
                            println!(
                                "  output: {}",
                                String::from_utf8_lossy(&output.stdout).trim()
                            );
                        }
                        Ok(output) => {
                            println!(
                                "end-to-end compile: FAIL (exit {})",
                                output.status.code().unwrap_or(-1)
                            );
                        }
                        Err(error) => println!("end-to-end compile: FAIL ({error})"),
                    }
                } else {
                    println!("end-to-end compile: FAIL (no executable produced)");
                }
            } else {
                println!("end-to-end compile: FAIL (no artifacts)");
            }
        }
        Ok(result) => {
            println!("end-to-end compile: FAIL");
            for diagnostic in result.diagnostics {
                if diagnostic.is_error() {
                    println!("  {}", diagnostic.message);
                }
            }
        }
        Err(error) => println!("end-to-end compile: FAIL ({error})"),
    }

    let _ = std::fs::remove_dir_all(&temp);

    if let Some(root) = project_root {
        if let Ok(project) = crate::project::load(root) {
            println!("project: OK");
            println!("  root: {}", project.root.display());
            println!("  name: {}", project.name);
            println!("  version: {}", project.version);
            println!("  entry: {}", project.entry.display());
            if !project.entry.is_file() {
                println!("  entry: MISSING");
            }
        } else {
            println!("project: not found or invalid");
        }
    }
}
