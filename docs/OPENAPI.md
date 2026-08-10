# Basis Tracker OpenAPI Documentation

The root `openapi.yaml` describes the current HTTP compatibility surface.

## Active API families

- Note creation and lookup.
- Reserve observation and owner-wallet reserve-payload admission.
- Event, status, acceptance-policy, and tracker-state observation.

Reserve construction remains fail-closed for v2 until its confirmed scanner,
BNS2/BRS2 state, and exact builder inputs are integrated. The tracker is not a
wallet proxy.

## Retired v1 redemption routes

The following compatibility routes are deprecated and always return
`410 Gone` before request-body or query parsing:

- `POST /redeem`
- `POST /redeem/complete`
- `GET /proof/redemption`
- `GET /tracker/proof`
- `GET /reserve/proof`
- `POST /tracker/signature`
- `POST /redemption/prepare`
- `POST /redemption/build`
- `POST /redemption/submit`

The generic `GET /proof` route does not exist. The tombstones expose no proof,
signer, transaction-construction, node-submission, broadcast, or settlement
state effect. V2 manifest admission is a separate dormant Rust boundary and is
not an HTTP route.

All tombstones return the standard error envelope:

```json
{
  "success": false,
  "data": null,
  "error": "Basis v1 redemption is retired; ..."
}
```

## Data conventions

- Public keys and signatures used by active note APIs are hex encoded.
- API responses use the `success`, `data`, and `error` envelope documented in
  `openapi.yaml`.
- The specification must be updated with route changes; it is not generated
  automatically from the Rust source.
