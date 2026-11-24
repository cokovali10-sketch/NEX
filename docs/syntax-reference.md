# Syntax Reference

NEX syntax is whitespace-sensitive and uses Python-style indentation.

## Statements
- `var name: Type = expr` (mutable variable)
- `let name: Type = expr` (immutable variable)
- `name = expr` (assignment)
- `return expr?` (return from function)
- `if cond:` (conditional with indented block)
- `while cond:` (loop with indented block)
- `for var in start..end:` (for loop)
- `for var in string:` (string iteration)
- `match expr:` (pattern matching)
- `unsafe:` (relaxed safety checks)
- `defer:` (cleanup on exit)

## Expressions
- Literals: `123`, `3.14`, `"hello"`, `'a'`, `true`, `false`
- Variables: `name`
- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `~`, `<<`, `>>`
- Call: `func(arg1, arg2)`
- Index: `array[index]`
- Field: `struct.field`
- Borrow: `&expr`, `&mut expr`
- Dereference: `*expr`
- Cast: `expr as Type`

## Types
- Scalars: `i64`, `f64`, `bool`, `string`
- Structs: `StructName { field: Type }`
- Enums: `EnumName.Variant`
- Arrays: `[Type]` (growable), `[Type; N]` (fixed)
- Tuples: `(Type, Type, ...)`
- Option: `Option<T>`
- Result: `Result<T, E>`