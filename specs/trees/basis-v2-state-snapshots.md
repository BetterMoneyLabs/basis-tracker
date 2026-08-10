# Basis v2 bounded state snapshots

## Status and scope

These formats persist the local state needed to reconstruct the fixed-shape
Basis v2 AVL commitments. They do not activate v2 HTTP routes, chain scanners,
transaction builders, contracts, networking, or reorganization handling.

Each data directory contains one Fjall partition, one authoritative snapshot
row, and one exclusive writer lease. A fresh directory is initialized only
with explicit `FreshV2StateApproval::Approve`. Existing v1, foreign, partial,
or ambiguous directories require an explicit migration or reset; they are
never converted in place.

All integers use unsigned big-endian encoding. All lengths are fixed except for
the bounded record sequence. The entry count is limited to 50,000 and the exact
snapshot length is checked before record allocation. Startup checks the
single-row partition invariant by reading at most two keys; it never performs
an unbounded row count before rejecting foreign or corrupt state.

## Full v2 claim record

Both snapshots embed the complete 244-byte signed claim in this order:

| Field | Bytes |
| --- | ---: |
| reserve NFT id | 32 |
| tracker NFT id | 32 |
| owner compressed public key | 33 |
| receiver compressed public key | 33 |
| asset kind (`0` ERG, `1` token) | 1 |
| token id, or all-zero for ERG | 32 |
| cumulative total debt | 8 |
| timestamp | 8 |
| Schnorr signature | 65 |

On every write and restart, the implementation reconstructs the
`ClaimDomainV2`, derives its 32-byte `claimKey`, reconstructs the `ClaimV2` with
`ClaimV2::from_signed`, and verifies the complete signature and domain. The
derived key, never a caller-supplied alias, is the AVL key and lookup authority.

## BNS2 tracker-claim snapshot

- Partition: `basis_v2_tracker_claims`
- Row key: `bns2_snapshot`
- AVL shape: 32-byte key, 8-byte cumulative-debt value

| Field | Bytes |
| --- | ---: |
| magic `BNS2` | 4 |
| ABI generation `2` | 1 |
| tracker NFT id | 32 |
| claim count | 4 |
| tracker AVL root | 33 |
| full claim records | `count * 244` |
| checksum | 32 |

The checksum is Blake2b-256 over
`"basis-v2-tracker-claims-bns2" || all preceding bytes`.

New claim keys append in first-insertion order. A valid monotone successor for
an existing key replaces the record at its original position. Restart rebuilds
`TrackerAvlTree` in that order and requires its root to equal the stored root.
Claims for another tracker NFT are rejected.

## BRS2 per-reserve redeemed-state snapshot

- Partition: `basis_v2_reserve_redeemed`
- Row key: `brs2_snapshot`
- AVL shape: 32-byte key, 24-byte `RedeemedStateV2` value
- Directory ownership: exactly one reserve NFT lineage and asset binding

| Field | Bytes |
| --- | ---: |
| magic `BRS2` | 4 |
| ABI generation `2` | 1 |
| tracker NFT id | 32 |
| reserve NFT id | 32 |
| asset kind (`0` ERG, `1` token) | 1 |
| token id, or all-zero for ERG | 32 |
| redeemed-state count | 4 |
| reserve AVL root | 33 |
| claim plus redeemed-state records | `count * (244 + 24)` |
| checksum | 32 |

The checksum is Blake2b-256 over
`"basis-v2-reserve-redeemed-brs2" || all preceding bytes`.

Every embedded claim must match the stored tracker NFT, reserve NFT, and asset.
Each 24-byte state is decoded by `RedeemedStateV2::decode` and must repeat the
claim timestamp and cumulative debt exactly. New keys append; successors retain
their first-insertion position. Restart rebuilds one independent
`ReserveAvlTree` for the bound reserve and requires the computed root to equal
the stored root.

The reserve store exposes reads only. Its raw state transition is private until
a confirmed-chain scanner owns the commit boundary, so a route or transaction
builder cannot directly advance redeemed progress.

## Durability and recovery

Every mutation is built and validated as a complete candidate in memory. The
single authoritative row is then replaced and persisted with Fjall
`PersistMode::SyncData`; only after that succeeds is the in-memory candidate
installed. An insert or durability error makes the live store terminally
poisoned because the durable outcome is unknown. Recovery requires dropping the
store and reopening the directory, which accepts only a fully valid old or new
snapshot.

The writer lease prevents concurrent in-process or cross-process writers for a
directory. Tracker and reserve directories are separate; each reserve NFT must
use its own BRS2 directory and root.
