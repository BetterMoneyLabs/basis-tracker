# Tracker Security Boundaries

This change makes wallet authority, signing authority, and settlement evidence
explicit. It intentionally breaks legacy conveniences that allowed a remote
caller to exercise tracker-owned capabilities or to assert settlement state.

## Enforced invariants

1. The HTTP service never forwards a reserve-creation request to the configured
   node wallet. `/reserves/create` remains a payload builder; the reserve owner
   reviews, signs, and submits that payload with their own wallet.
2. Change from tracker-signed fee inputs is paid only to the common P2PK script
   derived from the exact Sigma-serialized boxes used by the prover. Wallet-list
   JSON is selection metadata only; mismatched IDs, values, scripts, or assets
   are rejected, as are mixed-owner fee inputs.
3. Node API credentials and tracker signing material are redacted from `Debug`
   output. Signing and broadcast logs contain status and identifiers only, not
   request or response bodies.
4. A transaction artifact never contains private-key, mnemonic, seed, or
   `secrets` fields. The historical node-wallet artifact path is retired; local
   signing keeps the witness inside the signing boundary, and a missing witness
   never falls back to exporting it from the node wallet.
5. Node acceptance is not settlement confirmation. `/redemption/submit`
   accepts only the signed transaction and does not mutate note or reserve-tree
   accounting. `/redeem/complete` is a `410 Gone` tombstone.
6. Legacy `POST /redeem`, CLI `note redeem`, and MCP `note_redeem` are
   unconditional tombstones before account, network, construction, signing,
   broadcast, or persistence effects.
7. The exact committed Basis v2 ERG ErgoTree is recognized, but startup rejects
   its activation until the v2 scanner and BNS2/BRS2 state are installed. The
   historical identity remains only as a compatibility mode; reserve creation,
   server P2S distribution and HTTP redemption builders remain disabled. The
   older public library builder is a separate legacy-quarantine workstream.

## Compatibility changes

| Surface | New behavior |
| --- | --- |
| `POST /reserves/submit` | Returns `410 Gone`; no node-wallet request is made. |
| `reserve create --submit` | Returns an error; omit the flag and sign the payload in the owner wallet. |
| Assisted build `change_address` | Removed and rejected as an unknown field. |
| `POST /redemption/submit` | Accepts `{ "signed_tx": ... }`, returns `202 Accepted`, and performs no settlement mutation. |
| `POST /redeem/complete` | Returns `410 Gone`. |
| `note redeem` / MCP `note_redeem` | Return an error before effects; no boolean reactivation path remains. |
| Non-local `transaction generate-redemption` | Returns an error; use `--local-sign` or the assisted signer. |
| Server reserve P2S / HTTP builders | Historical identity: temporary compatibility only, not a safety endorsement. Exact v2 identity: recognized but startup-disabled. Unknown identity: rejected. Server construction/P2S distribution routes return `503 Service Unavailable`; legacy library builders are not covered by this branch. |

## Settlement hand-off

The confirmed-chain reconciler is the sole intended producer of settled local
state. It must authenticate the expected reserve successor, bind it to a block
on the selected active chain, apply the configured confirmation policy, and
support deterministic rollback before advancing note and reserve-tree state.
Until that reconciler is available, a submitted transaction remains only
node-accepted and must not be represented as settled.

## Regression boundary

Tests use sentinel strings and local data only. They prove that retired
handlers fail closed, caller accounting fields are rejected, fee change is
owner-bound, and `Debug`/artifact surfaces omit sentinel secrets. They do not
claim target-node admission, confirmation, reorg safety, or deployment parity.
