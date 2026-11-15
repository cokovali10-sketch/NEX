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
fn compound_assignment() {
    let source = r#"
fn main():
    var total = 0
    var i = 1
    while i <= 4:
        total += i
        i += 1
    print(total)
    var x = 10
    x -= 3
    x *= 2
    x /= 2
    x %= 5
    print(x)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "102");
}

#[test]
fn break_and_continue_in_for_loops() {
    let source = r#"
fn main():
    var sum = 0
    for i in 0..10:
        if i % 2 == 0:
            continue
        sum += i
    print(sum)

    var count = 0
    for i in 0..100:
        if i == 5:
            break
        count += 1
    print(count)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "255");
}

#[test]
fn string_equality() {
    let source = r#"
fn main():
    let a = "nex"
    let b = "nex"
    let c = "other"
    print(a == b)
    print(a != b)
    print(a == c)
    print(a != c)
    if a == "nex":
        print("-ok")
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "truefalsefalsetrue-ok");
}

#[test]
fn structs_work() {
    let source = r#"
struct Point:
    x: i64
    y: i64

fn main():
    var p = Point(1, 2)
    print(p.x)
    print(p.y)
    p.x = 10
    print(p.x + p.y)

    let q = p
    p.x = 99
    print(q.x)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "121210");
}

#[test]
fn struct_fields_can_be_mixed_scalar_types() {
    let source = r#"
struct Item:
    name: string
    price: f64
    in_stock: bool

fn main():
    var item = Item("tea", 2.5, true)
    print(item.name)
    print(item.price)
    print(item.in_stock)
    item.price = 1.25
    print(item.price)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "tea2.5true1.25");
}

#[test]
fn struct_errors_are_reported() {
    let unknown_field =
        compile("struct Point:\n    x: i64\n\nfn main():\n    var p = Point(1)\n    print(p.y)\n");
    assert!(
        unknown_field
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS027"),
        "{:?}",
        unknown_field.diagnostics
    );

    let immutable_field =
        compile("struct Point:\n    x: i64\n\nfn main():\n    let p = Point(1)\n    p.x = 2\n");
    assert!(
        immutable_field
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS007"),
        "{:?}",
        immutable_field.diagnostics
    );

    let recursive = compile("struct Node:\n    next: Node\n\nfn main():\n    print(1)\n");
    assert!(
        recursive
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS025"),
        "{:?}",
        recursive.diagnostics
    );

    let constant = compile(
        "struct Point:\n    x: i64\n\nconst P: Point = Point(1)\n\nfn main():\n    print(1)\n",
    );
    assert!(
        constant
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS028"),
        "{:?}",
        constant.diagnostics
    );

    let printed =
        compile("enum Color:\n    Red\n\nfn main():\n    let c = Color.Red\n    print(c)\n");
    assert!(
        printed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS015"),
        "{:?}",
        printed.diagnostics
    );
}

#[test]
fn string_ordering() {
    let source = r#"
fn main():
    let a = "apple"
    let b = "banana"
    print(a < b)
    print(a > b)
    print(a <= "apple")
    print(a >= "apple")
    print(b < a)
    if a < b:
        print("-less")
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "truefalsetruetruefalse-less");
}

#[test]
fn enums_and_match_work() {
    let source = r#"
enum Color:
    Red
    Green
    Blue

fn name(c: Color) -> string:
    match c:
        case Red:
            return "red"
        case Green:
            return "green"
        case Blue:
            return "blue"

fn main():
    print(name(Color.Red))
    print(" ")
    print(name(Green))
    print(" ")
    print(Color.Red == Red)
    print(Color.Red != Green)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "red green truetrue");
}

#[test]
fn match_fallthrough_and_else_work() {
    let source = r#"
enum Color:
    Red
    Green
    Blue

fn main():
    let c = Red
    match c:
        case Red:
            print("red")
        case Green:
            print("green")
    print("-done")

    let d = Blue
    match d:
        case Red:
            print("red")
        else:
            print("-other")
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "red-done-other");
}

#[test]
fn enum_errors_are_reported() {
    let unknown_variant = compile("enum Color:\n    Red\n\nfn main():\n    let c = Color.Blue\n");
    assert!(
        unknown_variant
            .diagnostics
            .iter()
            .any(|d| d.code == "NXS030"),
        "{:?}",
        unknown_variant.diagnostics
    );

    let duplicate_variant =
        compile("enum A:\n    Red\n\nenum B:\n    Red\n\nfn main():\n    print(1)\n");
    assert!(
        duplicate_variant
            .diagnostics
            .iter()
            .any(|d| d.code == "NXS029"),
        "{:?}",
        duplicate_variant.diagnostics
    );

    let non_enum_match = compile(
        "fn main():\n    let x = 1\n    match x:\n        case 1:\n            print(\"one\")\n",
    );
    assert!(
        non_enum_match
            .diagnostics
            .iter()
            .any(|d| d.code == "NXS031"),
        "{:?}",
        non_enum_match.diagnostics
    );

    let duplicate_arm = compile(
        "enum Color:\n    Red\n    Green\n\nfn main():\n    let c = Red\n    match c:\n        case Red:\n            print(\"r\")\n        case Red:\n            print(\"r2\")\n",
    );
    assert!(
        duplicate_arm.diagnostics.iter().any(|d| d.code == "NXS032"),
        "{:?}",
        duplicate_arm.diagnostics
    );

    let case_after_else = compile(
        "enum Color:\n    Red\n    Green\n\nfn main():\n    let c = Red\n    match c:\n        else:\n            print(\"e\")\n        case Red:\n            print(\"r\")\n",
    );
    assert!(
        case_after_else
            .diagnostics
            .iter()
            .any(|d| d.code == "NXS034"),
        "{:?}",
        case_after_else.diagnostics
    );
}

#[test]
fn arrays_work() {
    let source = r#"
fn main():
    let a = [1, 2, 3]
    print(a[0])
    print(a[1])
    print(a[2])

    var b = [10, 20, 30]
    b[1] = 99
    print(b[0])
    print(b[1])
    print(b[2])

    let c = b
    b[0] = 7
    print(c[0])
    print(b[0])

    print(a[0] + a[2])
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "1231099301074");
}

#[test]
fn array_errors_are_reported() {
    let empty = compile("fn main():\n    let a = []\n");
    assert!(
        empty.diagnostics.iter().any(|d| d.code == "NXS035"),
        "{:?}",
        empty.diagnostics
    );

    let bad_index = compile("fn main():\n    let a = [1, 2, 3]\n    print(a[true])\n");
    assert!(
        bad_index.diagnostics.iter().any(|d| d.code == "NXS036"),
        "{:?}",
        bad_index.diagnostics
    );

    let immutable = compile("fn main():\n    let a = [1, 2, 3]\n    a[0] = 9\n");
    assert!(
        immutable.diagnostics.iter().any(|d| d.code == "NXS007"),
        "{:?}",
        immutable.diagnostics
    );
}

#[test]
fn tuples_work() {
    let source = r#"
fn main():
    let t = (1, 2, 3)
    print(t.0)
    print(t.1)
    print(t.2)

    var u = (10, 20)
    u.0 = 99
    print(u.0)
    print(u.1)

    let v = u
    u.0 = 7
    print(v.0)
    print(u.0)

    print(t.0 + t.2)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "12399209974");
}

#[test]
fn modules_and_use_work() {
    let source = r#"
module math:
    pub fn add(a: i64, b: i64) -> i64:
        return a + b

    pub const PI: f64 = 3.14

    pub enum Color:
        Red
        Green

use math.add
use math.PI

fn main():
    print(add(2, 3))
    print(" ")
    print(PI)
    print(" ")
    print(math.Color.Red == math.Color.Red)
    print(" ")
    let c = math.Color.Green
    print(c == math.Color.Green)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "5 3.14 true true");
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
fn casts_work() {
    let source = r#"
fn main():
    let x: u8 = 255
    print(x)
    print(1.25 as f64)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "2551.25");
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
fn nested_struct_fields() {
    let source = r#"
struct Inner:
    x: i64
    y: i64

struct Outer:
    inner: Inner
    z: i64

fn main():
    var o = Outer(Inner(3, 4), 9)
    o.inner.x = 7
    print(o.inner.x)
    print(" ")
    print(o.inner.y)
    print(" ")
    o.inner = Inner(1, 2)
    print(o.inner.x)
    print(" ")
    print(o.inner.y)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "7 4 1 2");
}

#[test]
fn prints_aggregate_values() {
    let source = r#"
struct Point:
    x: i64
    y: i64

fn main():
    print(Point(3, 4))
    print(" ")
    print([1, 2, 3])
    print(" ")
    print((7, 8))
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "{3, 4} [1, 2, 3] (7, 8)");
}

#[test]
fn tuple_parameters_and_returns() {
    let source = r#"
fn identity(a: (i64, i64)) -> (i64, i64):
    return a

fn main():
    let a: (i64, i64) = (1, 2)
    var b = identity(a)
    b.1 = 9
    print(b.0)
    print(" ")
    print(b.1)
    print(" ")
    print(a.1)
    print(" ")
    let c: (i64, f64) = (10, 2.5)
    print(c.0)
    print(" ")
    print(c.1)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "1 9 2 10 2.5");
}

#[test]
fn array_parameters_and_returns() {
    let source = r#"
fn identity(a: [i64; 3]) -> [i64; 3]:
    return a

fn main():
    let a: [i64; 3] = [1, 2, 3]
    var b = identity(a)
    b[1] = 9
    print(b[0])
    print(" ")
    print(b[1])
    print(" ")
    print(a[1])
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "1 9 2");
}

#[test]
fn struct_parameters_and_returns() {
    let source = r#"
struct Point:
    x: i64
    y: i64

fn make(x: i64, y: i64) -> Point:
    return Point(x, y)

fn sum(p: Point) -> i64:
    return p.x + p.y

fn bump(p: Point) -> Point:
    var q = p
    q.x = q.x + 1
    return q

fn main():
    let p = make(3, 4)
    print(sum(p))
    print(" ")
    let q = bump(p)
    print(q.x)
    print(" ")
    print(p.x)
    print(" ")
    print(sum(Point(10, 20)))
    print(" ")
    let r = make(1, 2)
    print(r.y)
    print(" ")
    print(make(7, 8).x)
    print(" ")
    print(sum(make(5, 6)))
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "7 4 3 30 2 7 11");
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
fn private_module_item_is_reported() {
    let result = compile(
        "module math:\n    fn secret() -> i64:\n        return 1\n\nfn main():\n    print(math.secret())\n",
    );
    assert!(result.has_errors());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS038"),
        "{:?}",
        result.diagnostics
    );
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
