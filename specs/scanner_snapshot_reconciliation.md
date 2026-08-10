# Scanner Snapshot Reconciliation

## Scope

This change hardens the read-only node scanners used to discover reserve and
tracker boxes. It does not define rollback semantics, finality, or a durable
multi-store transaction journal.

## Invariants

1. Every indexed-node query supplies explicit `offset`, `limit`, ascending sort
   order, `includeUnconfirmed=false`, and `excludeMempoolSpent=false`. A local
   mempool spend is not a confirmed chain deletion.
2. A scan is complete only after every page succeeds, the final page is shorter
   than the requested page size, every box id is unique, and the indexed/full
   height pair is unchanged before and after pagination.
3. A lagging index, changed height pair, duplicate box id, malformed response,
   oversized response, request timeout, page failure, or configured scan bound
   produces an error. Such an error cannot trigger reserve reconciliation.
4. Reserve candidates are all parsed before the first persistent or in-memory
   mutation. A malformed candidate makes the snapshot unusable for
   reconciliation.
5. A successfully exhausted empty snapshot is authoritative for the indexed
   node observation and removes stale reserves only when the page response is
   exactly HTTP `200` with the JSON array `[]`. HTTP `404`, including on page
   zero, is an ambiguous source error and never means exhaustion. An incomplete
   or malformed snapshot never triggers reconciliation.
6. HTTP requests have connection and whole-request deadlines, response bodies
   are bounded before JSON parsing, page/item arithmetic is checked, and a
   shared semaphore bounds concurrent scanner requests. Dropping an in-flight
   request returns its permit before later work is admitted.

## Pinned Node Semantics

The API authority for this adapter is Ergo node v6.0.3 commit
`28ebb184b0c90ee9adebe1111eb6aa3244798ba9`.
`BlockchainApiRoute.scala` returns `Future[Seq[IndexedErgoBox]]` for both
`/blockchain/box/unspent/byAddress` and
`/blockchain/box/unspent/byTokenId/{tokenId}` through `ApiResponse`. At that
commit, `ApiResponse` emits HTTP `404` only for JSON `null`; an empty sequence
encodes as the non-null array `[]` and is emitted with HTTP `200`.

The OpenAPI file at the same commit also declares a `404` response with the
description "No unspent boxes found" for both routes. That declaration
conflicts with the route and response-wrapper implementation. The adapter does
not guess which upstream component produced a `404`: it fails closed. Only a
successfully parsed HTTP `200` array can contribute a page, and only `200 []`
can exhaust an empty page.

## Fixed Local Resource Policy

- Page size: 100 boxes.
- Maximum pages: 1,024.
- Maximum boxes per scan: 100,000.
- Maximum response body: 2 MiB.
- Maximum concurrent requests per scanner state: 4; excess work is rejected
  immediately rather than queued without a bound.
- Connect timeout: 5 seconds.
- Whole-request timeout: 15 seconds.

These are service limits, not Ergo consensus limits. A limit hit leaves the
previous derived state in place and requires an operator to raise the bound or
move to an application-owned block indexer.

## Closeout Matrix

| Invariant | Producer / enforcement | Downstream consumer | Failure if relaxed | Positive / isolated negative |
| --- | --- | --- | --- | --- |
| All pages are collected | Explicit offset loop and short-page exhaustion | Reserve acceptance and redemption discovery; tracker-box selection | A live box beyond page one disappears from derived state | `reserve_scan_paginates_past_explicit_page_size`, `tracker_scan_paginates_past_explicit_page_size` / `failed_later_page_preserves_previous_snapshot_without_partial_upserts` |
| One coherent indexed view is used | Caught-up indexed-height pair before/after scan | Reserve reconciliation | A moving or lagging index turns absence into deletion | Multi-page positives / `height_drift_preserves_previous_snapshot`, `indexed_height_lag_preserves_previous_snapshot_without_page_query` |
| Each observed box identity is unique | Cross-page `boxId` set | Snapshot membership | Offset drift can hide one live box behind a duplicate | Multi-page positives / `duplicate_across_pages_preserves_previous_snapshot` |
| Parsing completes before mutation | Full candidate parse phase | Persistent and in-memory reserve sets | One malformed candidate produces a partial or destructive view | Complete-empty and multi-page positives / `malformed_only_page_preserves_previous_snapshot` |
| Empty and incomplete are distinct | HTTP `200` array, short-page exhaustion, plus successful after-height probe | Stale reserve removal | Spent reserves remain authoritative, or live reserves are deleted after an ambiguous miss | `complete_empty_snapshot_removes_stale_reserves` / `reserve_page_zero_404_preserves_exact_previous_snapshot`, `tracker_later_page_404_is_error_without_partial_result`, page-failure, malformed, duplicate, lag and drift tests |
| Response work is bounded | Client deadlines, streaming byte budget, checked page/item limits | Scanner loop and service availability | A node stalls tasks or forces unbounded buffering | Normal page positives / `scanner_request_honors_whole_request_deadline`, both oversized-response tests |
| Concurrent work has no unbounded waiter queue | Shared four-permit `try_acquire` gate whose RAII permit is dropped with the request future | All scanner HTTP paths and clones | Concurrent callers accumulate indefinitely, or cancellation leaks capacity | Sequential/multi-page positives / `shared_gate_bounds_concurrent_scanner_requests`, `abandoned_request_returns_permit_for_exact_gate_capacity` |
| Cancellation cannot publish a partial observation | Collection and parsing precede every reserve mutation | Persistent `ReserveStorage` and in-memory `ReserveTracker` | Aborting a scan after page one publishes a mixed old/new view | Complete multi-page positive / `abort_during_second_page_preserves_exact_previous_snapshot` |
| Mempool observations do not delete confirmed state | Explicit `includeUnconfirmed=false`, `excludeMempoolSpent=false` | Confirmed reserve and tracker projections | A local pending spend erases a still-live confirmed reserve | Query assertion in `reserve_scan_paginates_past_explicit_page_size` |

## Persistence Boundary

The snapshot is validated before reconciliation, but `ReserveStorage` and
`ReserveTracker` remain separate stores without one atomic commit. A storage
failure during the apply phase can therefore leave a partially applied
snapshot. The state-journal workstream must provide the durable transaction or
replay receipt that closes that crash-consistency boundary.

Likewise, the height-pair check detects ordinary page drift but does not replace
header-id checkpoints, fork lineage, rollback, or finality policy. Those remain
dependencies of the state-journal/reorg workstream.
