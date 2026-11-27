# NEX libraries (30 modules)

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
- `seq.nex` — common reductions and transformations for `[i64]`
- `pathx.nex` — portable lexical path joining and component extraction
- `fsx.nex` — line-oriented file helpers built on whole-file I/O
- `envx.nex` — environment lookups and positional argument helpers
- `clix.nex` — `--flag` and `--option value` argument parsing
- `jsonx.nex` — JSON object/array accessors with defaults
- `mapx.nex` — safe map defaults, insert-if-absent, and integer counters
- `timex.nex` — monotonic elapsed time and seconds-to-milliseconds helpers
- `textx.nex` — lines, word counts, blank checks, and delimiter slicing
- `formatx.nex` — padding and grouped integer/percentage formatting
- `validate.nex` — ASCII identifiers, integer strings, and simple email syntax
- `urlx.nex` — query lookup and ASCII URL component encoding
- `csvx.nex` — standards-friendly field quoting and row encoding
- `htmlx.nex` — HTML text and element escaping
- `json_buildx.nex` — JSON object fields and string-array generation
- `seqstr.nex` — search, unique, and reverse for `[string]`
- `routerx.nex` — GET/POST registration and route result predicates
- `setx.nex` — unique-value operations for `[i64]`
- `optionx.nex` — optional-value helpers using `(present, value)` tuples
- `resultx.nex` — result helpers using `(ok, value, error)` tuples
- `configx.nex` — parse simple `key=value` config into a map
- `parsex.nex` — safe integer and boolean parsing into `(valid, value)` tuples
- `statsx.nex` — sum, mean, median, minimum, and maximum of `[i64]`
- `cookiex.nex` — lookup cookie values in an HTTP Cookie header
- `timefmt.nex` — readable compact durations from milliseconds
- `hashx.nex` — non-cryptographic DJB2 hash over characters
- `bytex.nex` — hexadecimal display for non-negative integers
- `httpx.nex` — text, HTML, and JSON HTTP response helpers

Run the original demo:

```
nexc libs/examples/demo.nex --emit exe
```

The foundation example imports the first reusable modules together:

```text
nex run libs/examples
```

To install one local library into a NEX project, run `nex add` with its `.nex`
file. Then import its short module name:

```powershell
nex add D:\path\to\NEX\libs\pathx.nex
```

```nex
use pathx from "pathx"
```

Paths are manipulated lexically; `pathx` does not query the filesystem or
resolve `.` and `..`. `fsx.read_lines` normalizes CRLF and retains the final
empty element when a file ends with a newline. Environment values that are set
to an empty string cannot be distinguished from missing values with the
current builtin API. `urlx.encode_component` preserves non-ASCII text as-is;
use a UTF-8-aware URL encoder for internationalized URL data. `csvx` currently
encodes fields and rows but does not parse quoted CSV. `validate.is_simple_email`
is only a syntax convenience, not an account or security check. `hashx` is not
cryptographic, and JSON/map handles use runtime allocations without an explicit
release function.

Public functions are marked `pub fn`. Private helpers stay module-local.
