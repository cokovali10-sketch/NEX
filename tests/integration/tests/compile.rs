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
    // The runtime prints "\n"; Windows text mode turns that into "\r\n".
    let normalize = |value: &[u8]| {
        String::from_utf8_lossy(value)
            .replace("\r\n", "\n")
            .replace('\r', "")
    };
    (
        normalize(&output.stdout),
        normalize(&output.stderr),
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

    let void_constant = compile("const P: void = 1\n\nfn main():\n    print(1)\n");
    assert!(
        void_constant
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "NXS002"),
        "{:?}",
        void_constant.diagnostics
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
        "fn main():\n    let x = 1.0\n    match x:\n        else:\n            print(\"x\")\n",
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
fn builtins_concat_and_to_string() {
    let source = r#"
fn main():
    print(concat("hello", " "))
    print(concat("world", "!"))
    print(" ")
    print(to_string(42))
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "hello world! 42");
}

#[test]
fn builtins_peek_and_poke() {
    let source = r#"
fn main():
    let p = alloc(8)
    poke(p, 42)
    print(peek(p))
    free(p)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "42");
}

#[test]
fn builtins_alloc_and_free() {
    let source = r#"
fn main():
    let p = alloc(16)
    print(p != 0)
    free(p)
    print(" done")
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "true done");
}

#[test]
fn builtins_assert_and_panic() {
    let source = r#"
fn main():
    assert(true)
    print("ok")
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "ok");

    let panicking = compile("fn main():\n    panic(\"boom\")\n");
    assert!(!panicking.has_errors());
    let executable = panicking
        .artifacts
        .as_ref()
        .and_then(|artifacts| artifacts.executable.clone())
        .expect("executable should be produced");
    let output = std::process::Command::new(&executable)
        .output()
        .expect("failed to run executable");
    assert!(!output.status.success());
}

#[test]
fn builtins_math() {
    let source = r#"
fn main():
    print(min(3, 9))
    print(" ")
    print(max(3, 9))
    print(" ")
    print(min(2.5, 1.25))
    print(" ")
    print(sqrt(9.0))
    print(" ")
    print(pow(2.0, 3.0))
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "3 9 1.25 3 8");
}

#[test]
fn builtins_len_and_abs() {
    let source = r#"
fn main():
    print(len("hello"))
    print(" ")
    let a: [i64; 4] = [10, 20, 30, 40]
    print(len(a))
    print(" ")
    print(abs(-7))
    print(" ")
    print(abs(5))
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "5 4 7 5");
}

#[test]
fn external_c_function() {
    let source = r#"
extern "C" fn puts(s: string) -> i32

fn main():
    let r = puts("ffi works")
    print(" ")
    print(r)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert!(stdout.contains("ffi works"));
    assert!(stdout.contains("0") || stdout.contains("1"));
}

#[test]
fn aggregate_equality() {
    let source = r#"
struct Point:
    x: i64
    y: i64

fn main():
    let p = Point(3, 4)
    let q = Point(3, 4)
    let r = Point(1, 2)
    print(p == q)
    print(" ")
    print(p != r)
    print(" ")
    let a: [i64; 3] = [1, 2, 3]
    let b: [i64; 3] = [1, 2, 3]
    let c: [i64; 3] = [1, 2, 4]
    print(a == b)
    print(" ")
    print(a != c)
    print(" ")
    let t = (7, 8)
    let u = (7, 8)
    let v = (7, 9)
    print(t == u)
    print(" ")
    print(t != v)
"#;
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "true true true true true true");
}

#[test]
fn aggregate_constants() {
    let source = r#"
struct Point:
    x: i64
    y: i64

const P: Point = Point(3, 4)
const A: [i64; 3] = [1, 2, 3]
const T: (i64, i64) = (7, 8)

fn main():
    print(P)
    print(" ")
    print(A)
    print(" ")
    print(T)
    print(" ")
    var p = P
    p.x = 99
    print(P.x)
    print(" ")
    print(p.x)
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "program exited with {code}, stderr: {stderr}");
    assert_eq!(stdout, "{3, 4} [1, 2, 3] (7, 8) 3 99", "stdout: {stdout}");
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
fn array_bounds_are_checked() {
    let result = compile("fn main():\n    let a: [i64; 3] = [1, 2, 3]\n    print(a[5])\n");
    assert!(!result.has_errors(), "{:?}", result.diagnostics);
    let executable = result
        .artifacts
        .as_ref()
        .and_then(|artifacts| artifacts.executable.clone())
        .expect("executable should be produced");
    let output = std::process::Command::new(&executable)
        .output()
        .expect("failed to run executable");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("out of bounds")
            || String::from_utf8_lossy(&output.stdout).contains("out of bounds")
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

#[test]
fn string_builtins() {
    let source = r#"
fn main():
    var text = "Hello NEX World"
    println(str_contains(text, "NEX"))
    println(str_contains(text, "Rust"))
    println(str_index_of(text, "NEX"))
    println(str_index_of(text, "zzz"))
    println(str_substring(text, 6, 9))
    println(str_upper("abc"))
    println(str_lower("ABC"))
    print("[")
    print(str_trim("  spaced  "))
    println("]")
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\nfalse\n6\n-1\nNEX\nABC\nabc\n[spaced]\n");
}

#[test]
fn string_builtin_type_errors() {
    let result = compile("fn main():\n    print(str_contains(\"a\", 1))\n");
    assert!(result.has_errors());
    assert!(result.diagnostics.iter().any(|d| d.code == "NXS004"));

    let result = compile("fn main():\n    print(str_upper(7))\n");
    assert!(result.has_errors());
    assert!(result.diagnostics.iter().any(|d| d.code == "NXS004"));

    let result = compile("fn main():\n    print(str_substring(\"abc\", 0))\n");
    assert!(result.has_errors());
    assert!(result.diagnostics.iter().any(|d| d.code == "NXS004"));
}

#[test]
fn utf8_bom_is_ignored() {
    let (stdout, stderr, code) = run_source("\u{feff}fn main():\n    print(\"bom ok\")\n");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "bom ok");
}

#[test]
fn pass_statement_is_a_no_op() {
    let source = "fn noop():\n    pass\n\nfn main():\n    noop()\n    print(\"ok\")\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "ok");
}

#[test]
fn pass_is_allowed_in_a_match_arm() {
    let source = "enum Color:\n    Red\n    Green\n\nfn main():\n    var c = Color.Red\n    match c:\n        case Red:\n            pass\n        case Green:\n            print(\"green\")\n    print(\"done\")\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "done");
}

#[test]
fn string_predicates_and_transformations() {
    let source = "fn main():\n    println(str_starts_with(\"hello\", \"he\"))\n    println(str_ends_with(\"hello\", \"lo\"))\n    println(str_ends_with(\"hello\", \"xx\"))\n    println(str_repeat(\"ab\", 3))\n    println(str_replace(\"a-b-c\", \"-\", \"+\"))\n    println(str_replace(\"aaa\", \"a\", \"bb\"))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\ntrue\nfalse\nababab\na+b+c\nbbbbbb\n");
}

#[test]
fn str_repeat_with_zero_returns_empty_string() {
    let source =
        "fn main():\n    print(\"[\")\n    print(str_repeat(\"x\", 0))\n    println(\"]\")\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "[]\n");
}

#[test]
fn string_helpers_parse_and_reverse() {
    let source = "fn main():\n    println(str_reverse(\"abc\"))\n    println(parse_i64(\"42\"))\n    println(abs(-2.5))\n    println(abs(-7))\n    println(to_string(2.5))\n    println(to_string(true))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "cba\n42\n2.5\n7\n2.5\ntrue\n");
}

#[test]
fn file_round_trip() {
    let dir = nex_backend::temp_dir("nex-file-io");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("note.txt");
    let path_string = path.to_string_lossy().replace('\\', "/");

    let source = format!(
        "fn main():\n    var p = \"{path_string}\"\n    println(file_exists(p))\n    println(write_file(p, \"hello\"))\n    println(file_exists(p))\n    print(read_file(p))\n    println(file_size(p))\n",
    );

    let (stdout, stderr, code) = run_source(&source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "false\ntrue\ntrue\nhello5\n");
}

#[test]
fn missing_file_reports_failure() {
    let source = "fn main():\n    println(file_exists(\"definitely-not-here.txt\"))\n    println(file_size(\"definitely-not-here.txt\"))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "false\n-1\n");
}

#[test]
fn tcp_client_reaches_a_local_listener() {
    use std::io::Read;
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local listener");
    let port = listener.local_addr().expect("local addr").port();

    let source = format!(
        "fn main():\n    var fd = tcp_connect(\"127.0.0.1\", {port})\n    println(fd > 0)\n    println(tcp_write(fd, \"ping\", 4) == 4)\n    println(tcp_close(fd))\n"
    );

    let (stdout, stderr, code) = run_source(&source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\ntrue\ntrue\n");

    let (mut stream, _) = listener.accept().expect("accept");
    let mut buffer = [0u8; 8];
    let read = stream.read(&mut buffer).expect("read");
    assert_eq!(&buffer[..read], b"ping");
}

#[test]
fn tcp_connect_to_a_closed_port_returns_negative() {
    let source = "fn main():\n    println(tcp_connect(\"127.0.0.1\", 1) < 0)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\n");
}

#[test]
fn http_request_parsing() {
    let source = "fn main():\n    var req = \"GET /hello HTTP/1.1\\r\\nHost: example.com\\r\\nContent-Type: text/plain\\r\\n\\r\\nbody-here\"\n    println(http_method(req))\n    println(http_path(req))\n    println(http_version(req))\n    println(http_header(req, \"host\"))\n    println(http_header(req, \"content-type\"))\n    println(http_body(req))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(
        stdout,
        "GET\n/hello\nHTTP/1.1\nexample.com\ntext/plain\nbody-here\n"
    );
}

#[test]
fn http_response_includes_length_and_close() {
    let source = "fn main():\n    print(http_response(\"200 OK\", \"text/plain\", \"hi\"))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.starts_with("HTTP/1.1 200 OK\n"), "{stdout}");
    assert!(stdout.contains("Content-Type: text/plain\n"), "{stdout}");
    assert!(stdout.contains("Content-Length: 2\n"), "{stdout}");
    assert!(stdout.contains("Connection: close\n\nhi"), "{stdout}");
}

#[test]
fn time_helpers_measure_and_sleep() {
    let source = "fn main():\n    println(unix_time() > 0)\n    var start = now_ms()\n    sleep_ms(25)\n    var elapsed = now_ms() - start\n    println(elapsed >= 20)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\ntrue\n");
}

#[test]
fn http_date_is_an_imf_fixdate() {
    let source = "fn main():\n    println(http_date())\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    let line = stdout.trim();
    let (weekday, rest) = line.split_once(", ").expect("comma after weekday");
    assert_eq!(weekday.len(), 3, "{line}");
    assert!(rest.ends_with(" GMT"), "{line}");
    assert_eq!(rest.len(), 24, "{line}");
}

#[test]
fn splitting_a_string_with_index_of_from() {
    let source = "fn main():\n    var line = \"host=example.com,port=8080\"\n    var comma = str_index_of_from(line, \",\", 0)\n    println(comma)\n    println(str_substring(line, 0, comma))\n    println(str_substring(line, comma + 1, len(line)))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "16\nhost=example.com\nport=8080\n");
}

#[test]
fn char_at_and_char_code() {
    let source = "fn main():\n    println(char_at(\"abc\", 1))\n    println(char_code(\"A\"))\n    println(char_at(\"abc\", 9))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "b\n65\n\n");
}

#[test]
fn growable_array_push_index_and_len() {
    let source = "fn main():\n    var a: [i64] = []\n    push(a, 10)\n    push(a, 20)\n    push(a, 30)\n    println(len(a))\n    println(a[0])\n    println(a[2])\n    a[1] = 99\n    println(a[1])\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "3\n10\n30\n99\n");
}

#[test]
fn growable_array_grows_past_doubling_boundaries() {
    let source = "fn main():\n    var a: [i64] = []\n    var i = 0\n    while i < 40:\n        push(a, i)\n        i = i + 1\n    println(len(a))\n    println(a[39])\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "40\n39\n");
}

#[test]
fn growable_array_out_of_bounds_panics() {
    let source = "fn main():\n    var a: [i64] = []\n    push(a, 1)\n    print(a[5])\n";
    let result = compile(source);
    assert!(!result.has_errors(), "{:?}", result.diagnostics);
    let executable = result
        .artifacts
        .as_ref()
        .and_then(|artifacts| artifacts.executable.clone())
        .expect("executable should be produced");
    let output = std::process::Command::new(&executable)
        .output()
        .expect("failed to run executable");
    assert!(!output.status.success(), "expected a non-zero exit");
}

#[test]
fn growable_array_of_strings() {
    let source = "fn main():\n    var a: [string] = []\n    push(a, \"one\")\n    push(a, \"two\")\n    println(len(a))\n    println(a[1])\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "2\ntwo\n");
}

#[test]
fn json_parse_and_query() {
    let source = "fn main():\n    var doc = json_parse(\"{\\\"name\\\": \\\"NEX\\\", \\\"tags\\\": [\\\"lang\\\", \\\"llvm\\\"], \\\"n\\\": 3, \\\"ok\\\": true}\")\n    println(json_type(doc))\n    println(json_str(json_get(doc, \"name\")))\n    println(json_type(json_get(doc, \"tags\")))\n    println(json_len(json_get(doc, \"tags\")))\n    println(json_str(json_at(json_get(doc, \"tags\"), 1)))\n    println(json_num(json_get(doc, \"n\")))\n    println(json_bool(json_get(doc, \"ok\")))\n    println(json_is_null(json_get(doc, \"missing\")))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "object\nNEX\narray\n2\nllvm\n3\ntrue\ntrue\n");
}

#[test]
fn json_invalid_input_returns_zero_handle() {
    let source = "fn main():\n    println(json_parse(\"{not json}\") == 0)\n    println(json_type(0) == \"\")\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\ntrue\n");
}

#[test]
fn split_builds_a_growable_array_in_nex() {
    let source = "fn split(text: string, sep: string) -> [string]:\n    var out: [string] = []\n    var i = 0\n    var total = str_split_count(text, sep)\n    while i < total:\n        push(out, str_split_at(text, sep, i))\n        i = i + 1\n    return out\n\nfn main():\n    var parts = split(\"a,b,c\", \",\")\n    println(len(parts))\n    println(parts[0])\n    println(parts[2])\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "3\na\nc\n");
}

#[test]
fn json_stringify_round_trips_with_escaping() {
    let source = "fn main():\n    var doc = json_parse(\"{\\\"a\\\": [1, 2, 3], \\\"b\\\": \\\"x\\\\ny\\\"}\")\n    println(json_stringify(doc))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "{\"a\":[1,2,3],\"b\":\"x\\ny\"}\n");
}

#[test]
fn map_set_get_has_remove_and_len() {
    let source = "fn main():\n    var m = map_new()\n    map_set(m, \"a\", \"1\")\n    map_set(m, \"b\", \"2\")\n    println(map_len(m))\n    println(map_get(m, \"a\"))\n    println(map_has(m, \"b\"))\n    println(map_has(m, \"zz\"))\n    map_set(m, \"a\", \"9\")\n    println(map_get(m, \"a\"))\n    println(map_len(m))\n    println(map_remove(m, \"a\"))\n    println(map_len(m))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "2\n1\ntrue\nfalse\n9\n2\ntrue\n1\n");
}

#[test]
fn map_handles_many_keys_and_enumeration() {
    let source = "fn main():\n    var m = map_new()\n    var i = 0\n    while i < 50:\n        var key = concat(\"k\", to_string(i))\n        map_set(m, key, to_string(i))\n        i = i + 1\n    println(map_len(m))\n    println(map_key_at(m, 0) != \"\")\n    var missing = 0\n    i = 0\n    while i < 50:\n        if not map_has(m, concat(\"k\", to_string(i))):\n            missing = missing + 1\n        i = i + 1\n    println(missing)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "50\ntrue\n0\n");
}

#[test]
fn thread_spawn_and_join_run_the_worker() {
    let source = "fn worker():\n    print(\"from-worker\")\n\nfn main():\n    var handle = thread_spawn(\"worker\")\n    println(handle > 0)\n    println(thread_join(handle))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    // The worker runs concurrently, so only its text and the join result are fixed.
    assert!(stdout.contains("from-worker"), "stdout: {stdout}");
    assert_eq!(stdout.matches("true").count(), 2, "stdout: {stdout}");
}

#[test]
fn thread_spawn_rejects_an_unknown_function() {
    let result = compile("fn main():\n    var h = thread_spawn(\"nope\")\n");
    assert!(result.has_errors());
    assert!(result.diagnostics.iter().any(|d| d.code == "NXS004"));
}

#[test]
fn mutex_lock_and_unlock_work() {
    let source = "fn main():\n    var lock = mutex_new()\n    println(lock > 0)\n    mutex_lock(lock)\n    print(\"in-critical-section\")\n    mutex_unlock(lock)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\nin-critical-section");
}

#[test]
fn globals_are_shared_between_functions() {
    let source = "var total = 0\n\nfn bump():\n    total = total + 1\n\nfn main():\n    bump()\n    bump()\n    println(total)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "2\n");
}

#[test]
fn two_threads_incrementing_a_global_with_a_mutex_do_not_race() {
    let source = "var counter = 0\nvar lock = 0\n\nfn bump():\n    var i = 0\n    while i < 1000:\n        mutex_lock(lock)\n        counter = counter + 1\n        mutex_unlock(lock)\n        i = i + 1\n\nfn main():\n    lock = mutex_new()\n    var a = thread_spawn(\"bump\")\n    var b = thread_spawn(\"bump\")\n    thread_join(a)\n    thread_join(b)\n    println(counter)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "2000\n");
}

#[test]
fn router_matches_paths_and_captures_params() {
    let source = "fn main():\n    var r = router_new()\n    router_add(r, \"GET\", \"/\")\n    router_add(r, \"GET\", \"/user/:id\")\n    router_add(r, \"POST\", \"/user/:id\")\n    println(router_match(r, \"GET\", \"/\"))\n    println(router_match(r, \"GET\", \"/user/42\"))\n    println(router_param(r, \"id\"))\n    println(router_match(r, \"POST\", \"/user/7\"))\n    println(router_param(r, \"id\"))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "1\n2\n42\n3\n7\n");
}

#[test]
fn router_reports_404_and_405() {
    let source = "fn main():\n    var r = router_new()\n    router_add(r, \"GET\", \"/user/:id\")\n    println(router_match(r, \"GET\", \"/nope\"))\n    println(router_match(r, \"PUT\", \"/user/1\"))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "0\n-1\n");
}

#[test]
fn thread_spawn_passes_an_i64_argument() {
    let source = "fn worker(times: i64):\n    print(times)\n\nfn main():\n    var a = thread_spawn(\"worker\", 41)\n    thread_join(a)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "41");
}

#[test]
fn threads_with_arguments_share_a_mutex_protected_counter() {
    let source = "var lock = 0\nvar total = 0\n\nfn worker(times: i64):\n    var i = 0\n    while i < times:\n        mutex_lock(lock)\n        total = total + 1\n        mutex_unlock(lock)\n        i = i + 1\n\nfn main():\n    lock = mutex_new()\n    var a = thread_spawn(\"worker\", 1000)\n    var b = thread_spawn(\"worker\", 1000)\n    thread_join(a)\n    thread_join(b)\n    println(total)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "2000\n");
}

#[test]
fn thread_pool_runs_every_submitted_task() {
    let source = "var lock = 0\nvar total = 0\n\nfn worker(times: i64):\n    var i = 0\n    while i < times:\n        mutex_lock(lock)\n        total = total + 1\n        mutex_unlock(lock)\n        i = i + 1\n\nfn main():\n    lock = mutex_new()\n    var pool = pool_new(4, 128)\n    println(pool > 0)\n    var i = 0\n    while i < 8:\n        println(pool_submit(pool, \"worker\", 1000))\n        i = i + 1\n    sleep_ms(300)\n    println(total)\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(
        stdout,
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n8000\n"
    );
}

#[test]
fn builder_matches_repeated_concat() {
    let source = "fn main():\n    var builder = builder_new()\n    println(builder_append(builder, \"ab\"))\n    println(builder_append(builder, \"cd\"))\n    println(builder_len(builder))\n    println(builder_string(builder))\n    builder_clear(builder)\n    println(builder_len(builder))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\ntrue\n4\nabcd\n0\n");
}

#[test]
fn builder_produces_the_same_string_as_concat() {
    let source = "fn main():\n    var a = \"\"\n    var b = builder_new()\n    var i = 0\n    while i < 2000:\n        a = concat(a, \"x\")\n        builder_append(b, \"x\")\n        i = i + 1\n    println(len(a) == builder_len(b))\n    println(builder_len(b))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "true\n2000\n");
}

#[test]
fn split_builds_a_string_array_in_nex() {
    let source = "fn split(text: string, separator: string) -> [string]:
    var out: [string] = []
    var i = 0
    var total = str_split_count(text, separator)
    while i < total:
        push(out, str_split_at(text, separator, i))
        i = i + 1
    return out

fn main():
    var parts = split(\"a,b,c\", \",\")
    println(len(parts))
    println(parts[0])
    println(parts[2])
    var query = split(\"id=7&page=2\", \"&\")
    println(len(query))
    println(query[1])
";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "3\na\nc\n2\npage=2\n");
}

#[test]
fn split_handles_empty_and_trailing_fields() {
    let source = "fn main():\n    println(str_split_count(\"a,b,c\", \",\"))\n    println(str_split_count(\"one\", \",\"))\n    println(str_split_at(\"a,b\", \",\", 5))\n";
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "3\n1\n\n");
}

#[test]
fn json_escape_quotes_backslashes_and_control_chars() {
    let source = r#"fn main():
    println(json_escape("a\"b\\c"))
    println(json_escape("line1\nline2"))
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "a\\\"b\\\\c\nline1\\nline2\n");
}

#[test]
fn built_json_parses_back() {
    let source = r#"fn main():
    var b = builder_new()
    builder_append(b, "{")
    builder_append(b, concat("\"n\": ", json_quote("a\"b")))
    builder_append(b, "}")
    var text = builder_string(b)
    println(text)
    var doc = json_parse(text)
    println(json_type(doc))
    println(json_str(json_get(doc, "n")))
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "{\"n\": \"a\\\"b\"}\nobject\na\"b\n");
}

#[test]
fn rounding_builtins() {
    let source = r#"fn main():
    println(floor(2.7))
    println(ceil(2.1))
    println(round(2.5))
    println(trunc(-2.9))
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "2\n3\n3\n-2\n");
}

#[test]
fn rounding_rejects_integers() {
    let result = compile("fn main():\n    print(floor(2))\n");
    assert!(result.has_errors());
    assert!(result.diagnostics.iter().any(|d| d.code == "NXS004"));
}

#[test]
fn trait_methods_use_static_dispatch() {
    let source = r#"
trait Shape:
    fn area(self) -> f64
    fn name(self) -> string

struct Circle:
    radius: f64

struct Rect:
    width: f64
    height: f64

impl Shape for Circle:
    fn area(self) -> f64:
        return 3.14 * self.radius * self.radius

    fn name(self) -> string:
        return "circle"

impl Shape for Rect:
    fn area(self) -> f64:
        return self.width * self.height

    fn name(self) -> string:
        return "rect"

fn main():
    var c = Circle(2.0)
    var r = Rect(3.0, 4.0)
    println(c.name())
    println(c.area())
    println(r.name())
    println(r.area())
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "circle\n12.56\nrect\n12\n");
}

#[test]
fn trait_methods_accept_parameters_and_call_functions() {
    let source = r#"
trait Counter:
    fn bump(self, by: i64) -> i64

struct Value:
    start: i64

fn base() -> i64:
    return 100

impl Counter for Value:
    fn bump(self, by: i64) -> i64:
        return self.start + by + base()

fn main():
    var value = Value(7)
    println(value.bump(5))
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "112\n");
}

#[test]
fn trait_errors_are_reported() {
    let missing = compile(
        "trait T:\n    fn run(self) -> i64\n\nstruct S:\n    x: i64\n\nimpl T for S:\n    fn other(self) -> i64:\n        return 1\n\nfn main():\n    print(1)\n",
    );
    assert!(
        missing.diagnostics.iter().any(|d| d.code == "NXS044"),
        "{:?}",
        missing.diagnostics
    );
    assert!(
        missing.diagnostics.iter().any(|d| d.code == "NXS045"),
        "{:?}",
        missing.diagnostics
    );

    let mismatch = compile(
        "trait T:\n    fn run(self) -> i64\n\nstruct S:\n    x: i64\n\nimpl T for S:\n    fn run(self) -> string:\n        return \"x\"\n\nfn main():\n    print(1)\n",
    );
    assert!(
        mismatch.diagnostics.iter().any(|d| d.code == "NXS044"),
        "{:?}",
        mismatch.diagnostics
    );

    let unknown_trait = compile(
        "struct S:\n    x: i64\n\nimpl Missing for S:\n    fn run(self) -> i64:\n        return 1\n\nfn main():\n    print(1)\n",
    );
    assert!(
        unknown_trait
            .diagnostics
            .iter()
            .any(|d| d.code == "NXS041"),
        "{:?}",
        unknown_trait.diagnostics
    );

    let unknown_method = compile(
        "trait T:\n    fn run(self) -> i64\n\nstruct S:\n    x: i64\n\nimpl T for S:\n    fn run(self) -> i64:\n        return 1\n\nfn main():\n    var s = S(1)\n    println(s.missing())\n",
    );
    assert!(
        unknown_method
            .diagnostics
            .iter()
            .any(|d| d.code == "NXS046"),
        "{:?}",
        unknown_method.diagnostics
    );
}

#[test]
fn pattern_matching_literals_runs() {
    let source = r#"
fn main():
    var x = 2
    match x:
        case 1:
            print("one")
        case 2:
            print("two")
        else:
            print("other")

    var s = "hi"
    match s:
        case "hello":
            print("greeting")
        case "hi":
            print("short")
        else:
            print("unknown")

    var c = 'b'
    match c:
        case 'a':
            print("A")
        case 'b':
            print("B")
        else:
            print("other")

    var flag = false
    match flag:
        case true:
            print("yes")
        case false:
            print("no")
"#;
    let (stdout, stderr, code) = run_source(source);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert_eq!(stdout, "twoshortBno");
}

#[test]
fn pattern_match_requires_else_for_scalars() {
    let integer = compile("fn main():\n    var x = 1\n    match x:\n        case 1:\n            print(\"one\")\n");
    assert!(integer.diagnostics.iter().any(|d| d.code == "NXS033"));

    let boolean = compile("fn main():\n    var x = true\n    match x:\n        case true:\n            print(\"yes\")\n");
    assert!(boolean.diagnostics.iter().any(|d| d.code == "NXS033"));
}
