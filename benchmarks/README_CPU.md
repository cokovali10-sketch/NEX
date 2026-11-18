# CPU benchmark (NEX vs Rust)

Purpose: measure generated code, with no sockets involved. Both programs run the
same three workloads and print timings in milliseconds.

- NEX: `examples/bench_cpu` (built with `nex build --release`)
- Rust: `benchmarks/cpu_rust` (`cargo build --release`)

Loop bodies carry data dependencies (LCG, sqrt chain) and bounds come from a
runtime clock, so the optimizer cannot close-form the workloads.

## Results, same run back to back (machine under load)

| Workload | NEX | Rust | ratio |
|---|---|---|---|
| `lcg_loop` — 20M iterations, 64-bit LCG | 106 ms | 108 ms | **1.0x** |
| `math_loop` — 5M × `sqrt(i)` | 15 ms | 0 ms | Rust vectorises |
| `string_loop` — 50 000 appends | 1 ms | 0 ms | parity |

Earlier runs on an idle machine gave the same picture (NEX 60 ms / Rust 59 ms for
the LCG loop). Absolute numbers on this machine move by up to 9x with load, so
**compare the ratio, not the milliseconds**.

## Correction to the previous version of this file

The first version measured `string_loop` as 50 000 × `concat(out, "x")` and
reported **711 ms for NEX vs 0 ms for Rust**. That number was real but it was
measuring the standard library, not the compiler: `concat` copies the whole
prefix on every call, so the loop was quadratic.

After adding `builder_append` (amortised O(1)), the same workload takes **1 ms**
and produces the same string. The old table is kept here only to explain the
difference; it is not a fair comparison of NEX and Rust codegen.

## Reading the numbers

**Integer code: parity.** 20M iterations of a dependent 64-bit multiply-add:
NEX 106 ms, Rust 108 ms. Same LLVM backend, same `-O2`. On scalar integer work
there is nothing to choose between them.

**Float loop: Rust wins by vectorising.** Rust turned the sqrt chain into SIMD;
NEX emitted a scalar loop at ~3 ns per `sqrt`, which is close to the libm call
cost. Closing this needs a vectorising pass over the NEX IR — not implemented.

**String building: parity, once the stdlib is fixed.** Both are amortised O(1).

## Honest conclusion

NEX generates code as fast as Rust for ordinary integer and string work. It is
slower on vectorisable float loops, and much slower on I/O-heavy servers because
of the server architecture, not the codegen.

The claim "faster than Rust" is not supported by these measurements, and I am not
making it.