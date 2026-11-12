# NEX Interoperability (Planned)

NEX is intended to interoperate with other languages through multiple levels.

## Level 1 - Native ABI

- Import and export C-compatible functions.
- Pass primitives, structs, arrays, and buffers.
- Callbacks and dynamic libraries.

## Level 2 - Runtime Adapters

Planned adapters:

- C and C++
- Rust
- Python
- Java and Kotlin
- C# and .NET
- Go
- Swift
- JavaScript and TypeScript
- Ruby, PHP, Lua, Zig, Fortran

Each adapter must define connection, type mapping, memory ownership, error
propagation, runtime version requirements, and limitations.

## Level 3 - Service Interoperability

For languages that cannot be linked directly:

- IPC
- RPC
- HTTP
- JSON
- Protocol Buffers
- WebAssembly
- Message queues

## Adapter SDK

Third-party adapters should be addable without modifying the compiler core.
An adapter is only supported when its integration tests pass.
