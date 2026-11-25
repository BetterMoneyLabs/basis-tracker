# Off-Chain (Client-Side) Redemption Signing

## Overview

A Basis redemption can be signed **entirely off-chain** and then broadcast to the node for
validation only. In this mode the node never produces any `proveDlog` proof: the receiver signs
the reserve input's `proveDlog(receiver)` locally, and the fee payer signs the fee input locally.
The tracker server only supplies the data it already knows — the receiver/issuer note, the AVL
proofs, and the tracker's own Schnorr signature over the debt record.

This is the signing model used by a TUI, a hardware wallet, or any client that must not delegate
signing to a hot node wallet. It is exercised by the CLI flag:

```bash
basis_cli transaction generate-redemption ... --local-sign
```

## Roles and Trust Boundaries

| Party | Knows | Produces |
|-------|-------|----------|
| Issuer (reserve owner) | issuer secret | issuer Schnorr signature over `key \|\| totalDebt \|\| timestamp` (context var `#2`) |
| Tracker server | tracker secret, AVL trees | tracker Schnorr signature (`#6`), reserve/tracker AVL proofs (`#5`, `#7`, `#8`), `totalDebt` |
| Receiver (note holder) | receiver secret | `proveDlog(receiver)` over the transaction bytes |
| Fee payer | fee-payer secret | `proveDlog(feePayer)` over the transaction bytes |
| Ergo node | (none required) | validates only; does not sign |

The issuer and tracker signatures are computed over the fixed 80-byte message
`blake2b256(ownerKey \|\| receiverKey) \|\| longToBytes(totalDebt) \|\| longToBytes(timestamp) \|\| trackerNftId`
(see [SCHNORR_SIGNATURE_SPEC.md](../SCHNORR_SIGNATURE_SPEC.md)). The `trackerNftId` (32 bytes) is
the tracker's NFT id, which the client learns from the note / tracker box; it binds the signature to
that tracker so it cannot be replayed on another. The signatures are independent of the
transaction. Only the two `proveDlog` proofs are computed over the transaction itself, which is
where off-chain signing must be byte-exact.

> **Scope of the tracker signature.** The signed message binds the note's `totalDebt` (and
> timestamp / tracker NFT id), but **not** the amount being redeemed in a given transaction.
> The acceptance policy and the FIFO ("oldest note first") ordering are therefore tracker-side
> **policy**, enforced by the tracker only when it agrees to issue its signature — they are not
> contract invariants. On-chain, the contract bounds every redemption at
> `totalDebt - redeemedDebt` for the spent reserve; a client that assembles the transaction
> itself (e.g. from `/tracker/proof` + `/reserve/proof` with an already-obtained signature) is
> constrained by that bound alone.

## ergo-lib Version Requirement

The deployed Basis reserve contract (`contract/basis.es`) uses the `Modulo`, `Exponentiate` and
`MultiplyGroup` ErgoScript opcodes when verifying the issuer/tracker Schnorr signatures. To reduce
the contract to its residual `proveDlog(receiver)` proposition, the client SDK must implement these
opcodes. In practice this requires **ergo-lib ≥ 0.28**; earlier versions (e.g. 0.13.x) fail during
signing with `NotImplementedOpCode` ("158…") while reducing the reserve script.

## Signed Message (`bytes_to_sign`)

For an Ergo transaction, `proveDlog` is computed over `bytes_to_sign`, the serialization of the
**unsigned** transaction: the count-prefixed inputs, the data inputs, the (de-duplicated) token
list, and the output candidates. Critically, each input is serialized as an `UnsignedInput`:

```
UnsignedInput = boxId (32 bytes) ++ ContextExtension (serialized)
```

Because the reserve input carries the redemption context variables (`#0`–`#8`) in its extension,
**the serialized context extension is part of `bytes_to_sign`**. Any difference in how that
extension is serialized — including the order in which the variables are emitted — changes the
signed message and therefore invalidates every `proveDlog` over the transaction.

This is why an extension-free, token-free payment serializes identically across implementations,
whereas a Basis redemption (which always carries a non-empty reserve extension) does not.

## Context Extension Serialization Order (critical)

The reference client (`sigma.interpreter.ContextExtension` in `sigmastate-interpreter`,
`data/shared/src/main/scala/sigma/interpreter/ContextExtension.scala`) stores the variables in a
`scala.collection.Map[Byte, EvaluatedValue]` and serializes them as:

```scala
w.putUByte(size)
obj.values.foreach { case (id, v) => w.put(id).putValue(v) }
```

i.e. it iterates the backing `Map` in **its own iteration order**, which for a `Map[Byte, _]` is
**not** insertion order — it depends on the indices (the `Byte` keys) via Scala's `HashMap` layout.
The JSON encoder (`org.ergoplatform.sdk.JsonCodecs.contextExtensionEncoder`) iterates the same map,
so the key order in a node-produced JSON object reflects the binary serialization order.

ergo-lib's `ContextExtension` is insertion-ordered (it preserves the order in which variables are
parsed/inserted). A Rust client that inserts variables as `0,1,2,3,4,5,6,8` therefore produces a
different `bytes_to_sign` than the node, and the node rejects the local `proveDlog` with:

```
HTTP 400 Malformed transaction: Scripts of all transaction inputs should pass verification.
<txid>: #0 => Success((false, 1273))
```

To sign off-chain, the reserve input's extension **must be serialized in the same order Scala
uses for that exact set of indices**. The order depends on the index *set* (Scala's `HashMap`
layout changes with size), so it must be determined per set.

### Known-good orders

| Redemption kind | Index set | Scala serialization order |
|-----------------|-----------|---------------------------|
| First redemption (no reserve lookup proof) | `{0,1,2,3,4,5,6,8}` | `0, 5, 1, 6, 2, 3, 8, 4` |
| Subsequent redemption (with `#7`) | `{0,1,2,3,4,5,6,7,8}` | `0, 5, 1, 6, 2, 7, 3, 8, 4` |

The first-redemption order was confirmed two ways: (1) read from the extension key order of a
node-signed redemption (the Scala JSON encoder iterates the same map as the binary serializer), and
(2) an ergo-lib tx-id parity test in which reordering the reserve extension to `0,5,1,6,2,3,8,4`
made `UnsignedTransaction::id()` byte-for-byte equal to the node's id for the same transaction.

The subsequent-redemption order was derived by reproducing Scala's `immutable.HashMap`
(HashTrieMap) iteration in code and self-validating the model against the confirmed
first-redemption order (the model reproduces `0,5,1,6,2,3,8,4` exactly for `{0,1,2,3,4,5,6,8}`,
so its output for `{0..8}` — `0,5,1,6,2,7,3,8,4` — is authoritative). The encoder is therefore
driven by the general algorithm rather than by a hardcoded per-set table.

### Implementation

In `crates/basis_cli/src/commands/transaction.rs`, the helper
`scala_context_extension_order(keys: &[u8])` reproduces Scala's `immutable.HashMap` (HashTrieMap)
iteration (the `improve` hash-mixing plus 5-bit trie, buckets iterated in ascending bit order) for
whichever indices are present, and `reorder_reserve_extension_scala(tx: &mut serde_json::Value)`
applies that order to `inputs[0].extension` immediately before the unsigned transaction is parsed
into an ergo-lib `UnsignedTransaction` and signed. Because ergo-lib parses the JSON object into its
map in key order, this makes the in-process serialization match the node exactly for both the
first-redemption set and the full `{0..8}` set.

## Diagnostic Methodology

The following no-spend techniques were used to isolate the failure and are reusable for any
client-side signer:

1. **`/transactions/check` validates without spending.** Submitting a fully-signed transaction to
   `POST /transactions/check` runs the node's script verification without adding it to the mempool.
   A node-signed control that passes `/transactions/check` proves the contract and fixtures are
   sound, isolating any failure to the locally-produced proof.

2. **Tx-id parity test.** `tx id = blake2b256(bytes_to_sign)`. Parsing the same unsigned-transaction
   JSON with the candidate signer and comparing its computed id to the node's id for the same
   transaction is a fast, no-spend check that `bytes_to_sign` matches. A mismatch means the proof
   will not verify, regardless of contract logic.

3. **Node-generated transaction comparison.** Generating a minimal transaction via
   `/wallet/transaction/generate` and comparing ids isolates whether a divergence is generic (affects
   even a plain payment) or specific to a feature (extension, tokens, registers).

4. **`/blocks/lastHeaders/10` for the state context.** ergo-lib 0.28 builds an `ErgoStateContext`
   from the last 10 headers plus chain parameters; the Basis contract does not read `CONTEXT.headers`
   or parameters, so the parameters may be left empty and only the `PreHeader` (height) matters.

## Confirmed Result

The first off-chain-signed redemption was confirmed on mainnet:

| Field | Value |
|-------|-------|
| Transaction ID | `c897018c1d59661769688feffddc2121923c64cf0769e4961f4b7c9f681558cd` |
| Inclusion height | 1826140 |
| Reserve input | `ddfd4223d3e6d3a9c4da0488b5daed0bbeaec51bcef9208ef4a756d5cbfecec1` (0.1 ERG) |
| New reserve output | `b4b2c78adff7651cdb23a6558da661658c8430529d6c6f65b1b2dd667a1e4118` (0.09 ERG) |
| Receiver output | `ac5af621b6c23bd84e64dff516def209b748495b1a93e36561da40ff266973fd` (0.01 ERG) |

See the "Fifth Redemption Test" section in
[redemption_execution_report.md](../redemption_execution_report.md) for the full execution details.

## Tracker-Assisted Variant (`/redemption/build` + `/redemption/submit`)

A second off-chain signing topology shifts more work to the tracker: the tracker's
`POST /redemption/build` endpoint builds the unsigned transaction (reserve-input
extension already in Scala order) around **caller-supplied fee inputs** — the redeemer
funds the miner fee (and, for token reserves, the recipient's min-ERG box) with their
own P2PK boxes, so the tracker pays and signs nothing in the transaction (issue #15
review N17). The build returns the unsigned transaction plus all signing material
(input/data box binaries, last 10 headers). The client signs every input — the reserve
input's `proveDlog(recipient)` and the fee inputs, all under the recipient key — over
the same `bytes_to_sign` and posts the fully-signed
transaction to `POST /redemption/submit`, which broadcasts it and syncs the tracker's
note/reserve-tree state (see
[redemption_state_spec.md](../server/redemption_state_spec.md#tracker-assisted-2-phase-endpoints-redemptionbuild-redemptionsubmit)).

Because the tracker produces the canonical extension order and the client never
re-serializes the extension, this variant avoids the reorder pitfall entirely. It is
implemented by `basis_cli transaction redeem-assisted` and by the TUI (`basis-ui`,
Notes → Redeem), and was confirmed on-chain twice (txs `e5a28452…` at height 1826973
and `85b14cc0…` at height 1827032 — see the "Sixth/Seventh Redemption Test" sections
in [redemption_execution_report.md](../redemption_execution_report.md)).

| Party | Produces in this variant |
|-------|--------------------------|
| Issuer (client) | issuer Schnorr signature over the 80-byte message (build request) |
| Tracker server | unsigned tx, all AVL proofs, tracker Schnorr signature (`#6`), broadcast, state sync |
| Receiver (client) | fee input boxes, `proveDlog(receiver)` over `bytes_to_sign` for the reserve input **and every fee input** |
| Ergo node | validates only; does not sign |

## Client-Side Verification (`basis_cli::verify`)

The CLI and TUI never sign or fund tracker-produced payloads unchecked (issue #15
review N6); `crates/basis_cli/src/verify.rs` gates both flows:

- **`verify_reserve_payload`** — before the owner's wallet funds a `/reserves/create`
  payload: exactly one request, the reserve contract compiled into the client for the
  denomination, the requested value and assets (NFT first, collateral token second),
  R4 = the owner key, R5 = the tracker's reserve-state digest for the
  (owner, denomination) pair (the empty tree for a fresh pair, the carried-forward
  redemption history after a rotation — `GET /reserves/state-digest`, N9), R6 = the
  expected tracker NFT id, R7 absent or 0, change address = the owner's P2PK, and
  `fee ≤ MAX_RESERVE_CREATION_FEE` (0.1 ERG). The last two stop a dishonest tracker
  from diverting the owner's change or burning the wallet on fees.
- **`verify_reserve_input`** — redemption input 0 is the issuer's reserve box under
  the compiled contract with the issuer's R4 (the client signs it with the creditor's
  key, so it must not be some other box whose script reduces to `proveDlog(creditor)`).
- **`verify_redemption_tx`** — before the creditor signs a `/redemption/build` result:
  the input/data boxes hash to the ids in the unsigned tx; input 0 is the reserve;
  every other input is one of the client-declared `fee_box_ids`, token-free and locked
  by the recipient's own P2PK; context var #0 redeems exactly `amount` into output 0;
  an output pays the recipient at least `amount`; every output pays the reserve
  contract, the recipient (payment or change), or the miner-fee contract capped at
  `MAX_REDEMPTION_FEE` (0.01 ERG); and ERG is conserved exactly (no value burned).
- **`redemption_issuer_signature`** — context var #2 is always the stored note's
  signature (verified locally against the 80-byte redemption message), never a fresh
  signature over tracker-supplied totals.

## References

- [redemption_transaction_format_spec.md](../server/redemption_transaction_format_spec.md) — transaction and context-extension layout
- [SCHNORR_SIGNATURE_SPEC.md](../SCHNORR_SIGNATURE_SPEC.md) — issuer/tracker 80-byte Schnorr signatures
- [redemption_cli_spec.md](../redemption_cli_spec.md) — CLI redemption command (node-signing path)
- sigmastate-interpreter: `sigma.interpreter.ContextExtension` (serializer), `org.ergoplatform.sdk.JsonCodecs` (JSON encoder)
