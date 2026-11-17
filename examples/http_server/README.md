# NEX HTTP server

A concurrent HTTP server written entirely in NEX: TCP accept loop, one thread per
connection, request parsing, routing with path parameters, and JSON responses.

```bash
nex run examples/http_server
# NEX server on http://127.0.0.1:8080
```

Routes:

| Method | Path | Response |
|---|---|---|
| `GET` | `/` | `{"hello": "world"}` |
| `GET` | `/user/:id` | `{"id": <id>}` |
| `POST` | `/items` | `created` |
| any | other path | `404` |
| wrong method | known path | `405` |

How it works: `main` accepts a connection and hands the socket to
`thread_spawn("serve", client)`. Each worker reads the request, dispatches it
through the router, writes the response and closes. Eight simultaneous requests
are served correctly and independently.

What it exercises: `tcp_*`, `http_*`, `router_*`, `thread_spawn` with an
argument, globals, `json`-less JSON responses built with `http_response`, `concat`,
`len`, `if`/`while`.

Known limits, stated on purpose:

- one `tcp_read` per request: a real server must loop until it sees the end of
  the body, so large POST bodies are truncated at 16 KB;
- no keep-alive: every response closes the connection;
- no static files and no JSON body parsing;
- the router is read-only after startup, and captured params are thread-local, so
  concurrent matches cannot overwrite each other's values.

The pieces for a full server — `threads`, `mutex`, `globals`, `json`, `map` — all
exist now; this example is the smallest honest use of them.