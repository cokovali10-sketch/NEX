# Performance

NEX is designed for good performance with predictable behavior:

## Compiler Performance
- Fast incremental builds (planned)
- Parallel compilation (planned)
- Large stack mode for deep recursion

## Runtime Performance
- Native code generation via LLVM
- No garbage collection (manual memory management)
- Stack-allocated values for small types
- Heap allocation only for strings and arrays

## Optimization Levels
- Debug: no optimizations, fast compile
- Release: standard optimizations
- Max: maximum optimizations, slower compile

## Memory Usage
- Strings and arrays grow as needed
- No automatic GC: use `drop()` to free memory
- Arena allocator for temporary allocations

## Benchmarks
See `benchmark-methodology.md` for detailed benchmarking approach and results.