# Getting Started

Create a project:

```powershell
nex new hello
cd hello
```

Edit `src/main.nex`:

```nex
fn main():
    print("Hello from NEX!")
```

Run it:

```powershell
nex run
```

Build an optimized executable:

```powershell
nex build --release
```

Type-check without building:

```powershell
nex check
```
