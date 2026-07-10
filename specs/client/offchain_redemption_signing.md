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

The issuer and tracker signatures are computed over the fixed 48-byte message
`blake2b256(ownerKey \|\| receiverKey) \|\| longToBytes(totalDebt) \|\| longToBytes(timestamp)`
(see [SCHNORR_SIGNATURE_SPEC.md](../SCHNORR_SIGNATURE_SPEC.md)) and are independent of the
transaction. Only the two `proveDlog` proofs are computed over the transaction itself, which is
where off-chain signing must be byte-exact.

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

## References

- [redemption_transaction_format_spec.md](../server/redemption_transaction_format_spec.md) — transaction and context-extension layout
- [SCHNORR_SIGNATURE_SPEC.md](../SCHNORR_SIGNATURE_SPEC.md) — issuer/tracker 48-byte Schnorr signatures
- [redemption_cli_spec.md](../redemption_cli_spec.md) — CLI redemption command (node-signing path)
- sigmastate-interpreter: `sigma.interpreter.ContextExtension` (serializer), `org.ergoplatform.sdk.JsonCodecs` (JSON encoder)
