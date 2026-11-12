use nex_backend::temp_dir;
use nex_driver::{CompileOptions, Emit, Profile, compile_source};
use std::process::Command;

fn compile(source: &str) -> nex_driver::CompileResult {
    let build_dir = temp_dir("nex-integration").join("build");
    compile_source(
        "test.nex",
        source.to_string(),
        CompileOptions {
            emit: Emit::Executable,
            profile: Profile::Debug,
            output: None,
            build_dir: Some(build_dir),
        },
    )
    .expect("toolchain should be available")
}

fn run_source(source: &str) -> (String, String, i32) {
    let result = compile(source);
    assert!(!result.has_errors(), "{:?}", result.diagnostics);
    let executable = result
        .artifacts
        .as_ref()
        .and_then(|artifacts| artifacts.executable.clone())
        .expect("executable should be produced");
    let output = Command::new(&executable)
        .output()
        .expect("failed to run executable");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn hello_world_runs() {
    let (stdout, _stderr, code) = run_source("fn main():\n    print(\"Hello from NEX!\")\n");
    assert_eq!(code, 0);
    assert_eq!(stdout, "Hello from NEX!");
}

#[test]
fn arithmetic_and_functions() {
    let source = r#"
fn add(a: i64, b: i64) -> i64:
    return a + b

fn main():
    print(add(2, 3))
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "5");
}

#[test]
fn variables_assignment_and_while() {
    let source = r#"
fn main():
    var total = 0
    var i = 1
    while i <= 5:
        total = total + i
        i = i + 1
    print(total)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "15");
}

#[test]
fn if_elif_else() {
    let source = r#"
fn classify(x: i64) -> i64:
    if x < 0:
        return 0
    elif x == 0:
        return 1
    else:
        return 2

fn main():
    print(classify(-4))
    print(classify(0))
    print(classify(9))
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "012");
}

#[test]
fn floats_bools_and_chars() {
    let source = r#"
fn main():
    print(3.5)
    print(true)
    print('A')
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "3.5trueA");
}

#[test]
fn main_can_return_i32() {
    let source = r#"
fn main() -> i32:
    return 7
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "");
    assert_eq!(code, 7);
}

#[test]
fn unsigned_and_casts_print() {
    let source = r#"
fn main():
    let x: u8 = 255
    print(x)
    print(1.25 as f64)
"#;
    // `as` is not implemented in 0.1, so this program must fail to compile.
    let result = compile(source);
    assert!(result.has_errors());
}

#[test]
fn division_by_zero_panics() {
    let source = r#"
fn main():
    print(1 / 0)
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(stdout, "");
    assert!(stderr.contains("division by zero"));
    assert_ne!(code, 0);
}

#[test]
fn syntax_error_is_reported() {
    let result = compile("fn main():\n    let x: = 1\n");
    assert!(result.has_errors());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXP006")
    );
}

#[test]
fn type_error_is_reported() {
    let result = compile("fn main():\n    let x: i64 = \"hi\"\n");
    assert!(result.has_errors());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS006")
    );
}

#[test]
fn immutable_assignment_is_reported() {
    let result = compile("fn main():\n    let x = 1\n    x = 2\n");
    assert!(result.has_errors());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS007")
    );
}

#[test]
fn missing_return_is_reported() {
    let result = compile("fn add(a: i64, b: i64) -> i64:\n    let x = a + b\n");
    assert!(result.has_errors());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS010")
    );
}

#[test]
fn emits_llvm_ir() {
    let build_dir = temp_dir("nex-integration-ir").join("build");
    let result = compile_source(
        "test.nex",
        "fn main():\n    print(\"ir\")\n".to_string(),
        CompileOptions {
            emit: Emit::LlvmIr,
            profile: Profile::Debug,
            output: None,
            build_dir: Some(build_dir),
        },
    )
    .expect("toolchain should be available");
    assert!(!result.has_errors());
    assert!(
        result
            .llvm_ir
            .as_ref()
            .unwrap()
            .contains("define i32 @main()")
    );
}

#[test]
fn emits_object_file() {
    let build_dir = temp_dir("nex-integration-obj").join("build");
    let result = compile_source(
        "test.nex",
        "fn main():\n    print(\"obj\")\n".to_string(),
        CompileOptions {
            emit: Emit::Object,
            profile: Profile::Debug,
            output: None,
            build_dir: Some(build_dir),
        },
    )
    .expect("toolchain should be available");
    assert!(!result.has_errors());
    let artifacts = result.artifacts.unwrap();
    assert!(artifacts.object.unwrap().is_file());
    assert!(artifacts.runtime_object.unwrap().is_file());
}

#[test]
fn literal_range_check() {
    let result = compile("fn main():\n    let x: u8 = 300\n");
    assert!(result.has_errors());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS021")
    );
}
