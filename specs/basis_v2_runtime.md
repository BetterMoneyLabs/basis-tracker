# Basis v2 runtime boundary

## Contract identity

The runtime recognizes one exact ERG reserve contract and one exact token
reserve contract for ABI generation 2. Their full ErgoTree bytes are committed
under `crates/basis_store/contracts/` and are compared byte-for-byte after a
configured P2S address is decoded.

| Family | ErgoTree bytes | SHA-256 |
| --- | ---: | --- |
| ERG | 1,682 | `2690634924efb22359a776f89f5274d77e067bd8ad0619a6e358a2f96697a0c2` |
| token | 1,963 | `ba1df64e7d95ecffc4f3d49fcada8baebe59a676eb617737cd010bdb52381cb3` |

The bytes originate from ChainCash contract source/golden commit
`9a274396d5f78f7be5ed76bacee5329c42570317`. A readable label, an unknown P2S,
or either v1 generation is not an admissible substitute.

## Claim identity

A v2 note is not keyed solely by `(issuer, receiver)`. Its 32-byte claim key is:

```text
ERG:
blake2b256(
  "BASIS" || 2 || mainnet || ERG ||
  reserveNft || trackerNft || owner || receiver
)

token:
blake2b256(
  "BASIS" || 2 || mainnet || token ||
  reserveNft || reserveToken || trackerNft || owner || receiver
)
```

The debtor and tracker sign the same 48-byte message:

```text
claimKey || totalDebt:i64-be || timestamp:i64-be
```

The Rust API represents monetary values as `u64`, but rejects zero and values
above `Long.MaxValue` before serialization. Compressed public keys are parsed
before a domain is constructed. For token reserves, the singleton id and
reserve-token id must differ.

## Authenticated tree shapes

ABI v2 uses two distinct authenticated-state families:

| State | Namespace | Key | Value | Operations |
| --- | --- | ---: | ---: | --- |
| tracker R5 | global v2 claim ledger | 32-byte claim key | 8-byte cumulative debt | insert + update, no remove |
| reserve R5 | one lineage per reserve NFT | 32-byte claim key | 24-byte `(timestamp,totalDebt,redeemed)` | insert + update, no remove |

Reserve state must never be held in one process-global AVL root. Every reserve
NFT has an independent root, proof history, active box lineage and restart
checkpoint.

For an existing reserve entry, the same signed `(timestamp,totalDebt)` may
advance `redeemed`. A newer timestamp may increase, but never decrease,
`totalDebt`. Membership and non-membership proofs are both mandatory.

## Bounded manifest admission

Untrusted manifest JSON enters through one raw-byte parser capped at 2 MiB.
`V2RedemptionManifest` is serializable but is not directly deserializable, so a
CLI or future transport cannot bypass that allocation boundary. Every top-level
field is required; branch-absent evidence is represented by an explicit JSON
`null`, not by omitting the key.

Before hex decoding, the runtime enforces these limits:

| Field family | Encoded limit |
| --- | ---: |
| fixed identifiers, keys, roots, signatures, states | exactly twice the fixed byte width |
| AVL proof | non-empty, even-length hex; at most 131,072 characters / 65,536 bytes |
| exact Sigma box | non-empty, even-length hex; at most 8,192 characters / 4,096 bytes |

Funding is limited to 1-16 exact token-free boxes under one P2PK owner. Every
transaction output, including the reserve successor, creditor payout, miner
fee, and optional owner change, must be within
`1,000,000..=Long.MaxValue` nanoERG. Funding and transaction aggregates use
checked arithmetic; a per-box-valid set whose aggregate overflows is rejected.

## Activation boundary

The exact v2 identity, state primitives, manifest builder, and signer-side
validator exist, but production activation remains deliberately unavailable.
Confirmed boxes and signer tips have no public production constructor; a test
height or caller boolean cannot create chain authority. No server, CLI, or TUI
route can prove, sign, submit, or broadcast a v2 redemption.

Activation requires a sealed header-ancestry reconciler, authoritative
BNS2/BRS2 state, and a private prover/signer that consumes only the validated
manifest and supplies all of the following as one coherent transaction:

- R4-R9 reserve registers, including immutable emergency R8 and predecessor R9;
- fixed-width tracker and reserve AVL trees and mandatory context variables
  `0..8` as required by the selected branch;
- the exact reserve-bound claim key and both signatures;
- one payout immediately after the successor, with P2PK receiver, exact amount
  and R4 equal to the reserve input id;
- fee inputs and change outside reserve accounting;
- reserve-NFT-specific proof/root/box lineage and an idempotent confirmed-chain
  settlement record.

Until that join exists, v2 stays dormant. The nine legacy HTTP compatibility
routes return `410 Gone`; the global v1 reserve AVL/proof actor surface and the
v1 transaction builder, signer, navigation, and local-sign fixture are removed.
The current admission layer implements no prover, wallet integration,
submission, broadcast, settlement reconciliation, or migration.

## Coexistence and migration

There is no implicit conversion of a bilateral v1 note into a reserve-bound v2
claim, and the v2 contract cannot mutate existing v1 boxes. Activation
therefore requires an explicit generation manifest, fresh v2 reserves, and a
separately approved support/sunset policy for attributable historical state.
