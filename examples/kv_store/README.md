# kv_store

A small JSON HTTP key-value service written in NEX. It demonstrates that NEX
can host a real networked service: a TCP accept loop, a router with path
parameters, a worker pool, a mutex-guarded in-memory map, a read-only JSON
DOM and `http_response` output.

## Run

```powershell
nex run examples/kv_store
```

The server listens on `127.0.0.1:8080`. In another terminal:

```powershell
# liveness probe
curl http://127.0.0.1:8080/kv/health

# store a key
curl -X POST http://127.0.0.1:8080/kv/set -d '{"key":"apple","value":"red"}'

# read it back
curl http://127.0.0.1:8080/kv/get/apple        # {"value":"red"}
curl http://127.0.0.1:8080/kv/get/missing      # 404 {"error":"key not found"}
curl -X POST http://127.0.0.1:8080/kv/set -d 'oops'   # 400 {"error":"invalid JSON body"}
```

## Design

- `read_request` loops over `tcp_read` until the request head
  (`\r\n\r\n`) is complete and reads any `Content-Length` body bytes.
- `dispatch` matches the method and path with `router_match`.
  `-1` means the path exists under another method (`405`).
- The map is shared between worker threads, so every access is bracketed by
  `mutex_lock` / `mutex_unlock`.
- Responses are built with `json_quote` for string escaping; the JSON DOM is
  read-only, so building the response is done with `concat`.

Every builtin used here is implemented in `runtime/core/nex_rt.c` and
type-checked by the compiler; nothing is stubbed.

## Limitations

- The in-memory map has no eviction and is lost on exit.
- Concurrency is a thread pool with one blocking handler per connection;
  there is no async runtime yet.
- `json_parse` handles `object`, `array`, `string`, `number`, `bool` and
  `null`; it is a read-only DOM.
