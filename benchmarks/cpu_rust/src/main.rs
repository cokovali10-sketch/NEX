//! CPU-bound workloads, identical to `examples/bench_cpu` so NEX and Rust run
//! exactly the same work and print the same timings in milliseconds.
//!
//! Loop bodies carry a data dependency, so the optimizer cannot close-form them.

const ITER: i64 = 20_000_000;

fn lcg_loop(seed: i64) -> i64 {
    let mut x = seed;
    let mut i: i64 = 0;
    while i < ITER {
        x = (x * 1103515245 + 12345) % 2147483648;
        i += 1;
    }
    x
}

fn math_loop() -> f64 {
    let mut total: f64 = 0.0;
    let mut i: f64 = 1.0;
    let limit: f64 = 5_000_000.0;
    while i < limit {
        total += i.sqrt();
        i += 1.0;
    }
    total
}

fn string_loop() -> String {
    let mut out = String::new();
    for _ in 0..50_000 {
        out.push('x');
    }
    out
}

fn main() {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);

    let start = std::time::Instant::now();
    let a = lcg_loop(seed);
    let mid = start.elapsed().as_millis() as i64;
    let b = math_loop();
    let late = start.elapsed().as_millis() as i64;
    let c = string_loop();
    let end = start.elapsed().as_millis() as i64;

    println!("lcg_loop: {}", mid);
    println!("math_loop: {}", late - mid);
    println!("string_loop: {}", end - late);
    println!("checksum: {}", a + c.len() as i64);
    let _ = b;
}