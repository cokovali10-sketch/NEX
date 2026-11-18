## Measured results

Two measurements, taken hours apart on the same machine. They differ by ~9x
because the machine load changed, so **compare the ratio, not the absolute
numbers**.

| Run | NEX req/s | axum req/s | ratio |
|---|---|---|---|
| machine idle | 2 338 | 47 377 | 20x |
| machine loaded | 340 | 5 279 | 15x |

Same endpoint, same load generator (50 connections), same `-O2` for both (NEX
maps `Profile::Release` to `-O2`).

## What was tried and did not help: keep-alive

I implemented `Connection: keep-alive`, a read loop that returns to `recv` after
each response, and `tcp_set_timeout` (`SO_RCVTIMEO`) plus `TCP_NODELAY` so pool
workers would not block forever on idle sockets.

| Server variant | req/s | p99 | failed |
|---|---|---|---|
| pool of 8, close per request | 2 338 | 30 ms | 0 |
| pool of 8, keep-alive + 1 s timeout | 263 | 270 ms | 0 |
| pool of 64, keep-alive | 368 | 271 ms | 0 |

Keep-alive made it **worse**, not better. The likely reason is that a pooled
worker sits inside `recv` on an idle keep-alive connection, so with 8 workers and
50 concurrent connections most workers are parked; raising the pool to 64 helped
only marginally. I did not prove this beyond the measurements above, so treat it
as a hypothesis.

The honest conclusion: **the bottleneck is per-request work, not connection
setup.** Optimising connection reuse without fixing the per-request path (path
splitting with `malloc`, `snprintf` response building) does not move the number.

I reverted keep-alive. `tcp_set_timeout` stays available in the stdlib; the
example server does not use it.

## Where the remaining gap is

1. **Per-request allocation**: the router splits the path into owned strings and
   `http_response` builds the response with `snprintf`. A router that matches
   without allocating is the next real win.
2. **One `tcp_read` per request**, so a request split across packets needs the
   loop that `read_request` now has.
3. **CPU-bound work is not measured here.** For hashing, parsing, or template
   rendering the gap should be much smaller, because both go through the same
   LLVM. That measurement is the next one worth doing.