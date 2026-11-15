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
    let normalize = |value: &[u8]| String::from_utf8_lossy(value).replace("\r\n", "\n");
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
    let (stdout, _stderr, code) = run_source(source);
    assert_eq!(code, 0);
    assert_eq!(stdout, "{3, 4} [1, 2, 3] (7, 8) 3 99");
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
    assert!(stdout.starts_with("HTTP/1.1 200 OK\r\n"), "{stdout}");
    assert!(stdout.contains("Content-Type: text/plain\r\n"), "{stdout}");
    assert!(stdout.contains("Content-Length: 2\r\n"), "{stdout}");
    assert!(stdout.contains("Connection: close\r\n\r\nhi"), "{stdout}");
}
