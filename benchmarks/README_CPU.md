# CPU benchmark (NEX vs Rust)

Purpose: measure generated code, with no sockets involved. Both programs run the
same three workloads and print timings in milliseconds.

- NEX: `examples/bench_cpu` (built with `nex build --release`)
- Rust: `benchmarks/cpu_rust` (`cargo build --release`)

The loops carry data dependencies (LCG, sqrt chain) so the compiler cannot
close-form them; loop bounds depend on a runtime clock value.

## Results on this machine (Windows 11, release builds, best of 3)

| Workload | NEX | Rust | ratio |
|---|---|---|---|
| `lcg_loop` — 20M iterations, 64-bit LCG | 60 ms | 59 ms | **1.02x** |
| `math_loop` — 5M × `sqrt(i)` | 9 ms | 0 ms | Rust optimised it away |
11 | `string_loop` — 50 000 × `concat(out, "x")` | 711 ms | 0 ms | NEX is quadratic here |

## Reading the numbers

**Integer code: parity.** 20M iterations of a dependent 64-bit multiply-add take
60 ms in NEX and 59 ms in Rust. That is the honest headline: on the same LLVM
backend, NEX-generated code is indistinguishable from Rust for scalar integer
work.

**Float loop: Rust wins by vectorising.** Rust turned the sqrt chain into SIMD
(or folded it); NEX emitted a scalar loop at 1.8 ns per `sqrt`, which is already
close to the libm call cost. Closing this needs a vectorising pass over the NEX
IR, which is not implemented.

**String building: NEX is quadratic.** `concat` allocates a new buffer and copies
the old contents every call, so 50 000 concatenations copy ~1.25 GB in total
(711 ms). Rust's `String::push` is amortised O(1) (0 ms). This is not a codegen
problem — it is a stdlib design problem, and it is the next thing worth fixing.

## What this means

- NEX can generate code as fast as Rust for ordinary integer work. The claim
  "faster than Rust" is not supported by these numbers, and I am not making it.
- The places NEX loses are (1) string allocation strategy, (2) vectorisation,
  (3) the HTTP server architecture — not the compiler's per-statement output.
- The next fix with the best ratio of effort to gain is a growable string buffer
  so `concat` stops being quadratic.