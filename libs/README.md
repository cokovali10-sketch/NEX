# NEX standard-ish libraries

Reusable modules written in NEX. Import with `use`, call functions with the
module prefix.

```nex
use mathx from "libs/mathx.nex"

fn main():
    println(to_string(mathx.gcd(48, 36)))   # 12
```

## Modules

- `mathx.nex` — `square`, `cube`, `clamp`, `gcd`, `max_of`, `min_of`
- `strx.nex` — `repeat`, `shout`, `surround`

Run the demo:

```
nexc libs/examples/demo.nex --emit exe
```

Public functions are marked `pub fn`. Private helpers stay module-local.