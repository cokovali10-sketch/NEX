# NEX HTTP server

A tiny HTTP server written entirely in NEX: TCP accept loop, request parsing,
routing with path parameters, and JSON responses.

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

What it exercises: `tcp_listen`, `tcp_accept`, `tcp_read`, `tcp_write`,
`http_method`, `http_path`, `http_response`, `router_*`, `concat`, `len`,
globals, `if`/`while`.

Known limits, stated on purpose:

- one connection at a time: `thread_spawn` cannot receive arguments yet, so a
  per-connection worker cannot be passed the socket;
- one `tcp_read` per request: a real server must loop until it sees the end of
  the request body;
- no keep-alive, no static files, no JSON body parsing here.

`threads` and `mutex` exist, so the concurrency pieces are in place; wiring a
worker per connection is the next step.