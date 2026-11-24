# Memory Model

NEX 1.0 uses a conservative, conservative ownership and borrowing model.

- Move semantics: strings and growable arrays are moved when assigned or passed
  to a function; moved-from values cannot be used again.
- Partial borrow checking: `&T` and `&mut T` reference types, but they can only
  be passed directly to function calls and cannot be stored in variables.
- No lifetimes: references passed to functions must outlive the call, but
  this is not checked yet.
- **RAII for arrays**: `drop()` frees arrays and strings; strings from literals
  are static and never freed.