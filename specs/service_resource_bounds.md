# Service Resource Bounds

This document defines the fail-closed resource limits applied by the Basis
server. They protect availability; they do not authenticate node responses or
make point-in-time policy decisions atomic with later issuance.

## Inbound state and pagination

- The in-memory event log retains at most 10,000 events. Adding a new event
  evicts the oldest entry.
- Event pages contain 1 through 100 entries. Offset multiplication and event ID
  increments use checked arithmetic; an offset past the retained window returns
  an empty page.
- Issuer outstanding-debt aggregation and cumulative redeemed amounts use
  checked addition and reject overflow.

## Tracker actor requests

- HTTP handlers use non-blocking `try_send`; a full or closed command queue is
  rejected rather than awaited.
- The response deadline is five seconds. When the requester has expired, the
  single tracker worker drops the command before starting its work.
- Work that already started is not asynchronously interrupted. It remains
  serialized by the single worker, so this bound prevents an unbounded queued
  backlog rather than promising cancellation of an in-progress state operation.

## Outbound node HTTP

All node calls made by the API, redemption builder, and tracker-box updater use
one process-wide bounded client:

- at most 16 concurrent requests, acquired without waiting;
- a 15-second total request deadline and a three-second connection cap;
- at most 2 MiB of response body, enforced while reading chunks even when the
  server omits `Content-Length`;
- bounded error bodies and JSON parsing only after the body cap succeeds.

The negative matrix covers a stalled loopback server, a saturated permit set,
and a chunked response that crosses the cap. No test contacts a live Ergo node.

## Integration boundary

The state-journal and confirmed-settlement workstreams may replace tracker
commands and updater sequencing. They must preserve these queue and HTTP bounds
when integrating their actor-owned publication lease. Reorg handling, durable
settlement evidence, inbound authentication, and operator-configurable limits
remain separate workstreams.
