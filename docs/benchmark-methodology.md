# Benchmark Methodology

NEX benchmarks use a consistent methodology:

- Warm-up: 3 iterations
- Measure: 10 iterations
- Average reported in milliseconds
- Same machine for all comparisons (Intel i7-11800H)
- Windows x86_64, release builds
- NEX: `nex build --max` or `nexc --max`
- Rust: `cargo build --release`

Benchmarks include:
- Math loop (simple arithmetic)
- String operations (concat, search, format)
- Array operations (push, pop, map)
- JSON parsing/serialization
- HTTP server throughput

Results are checked for reproducibility across runs.