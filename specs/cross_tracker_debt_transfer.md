# Cross-Tracker Debt Transfer

## Summary

This document describes how a Basis debt obligation that is tracked by one tracker can be moved to a different tracker system. It compares three mechanisms:

1. **Off-chain cross-tracker novation** — no contract changes, requires cooperation of both trackers.
2. **On-chain redemption + re-issuance** — uses the existing reserve contract to settle the source debt and create a new note under the target tracker.
3. **Multi-tracker reserve gateway** — a contract extension that lets one reserve back notes from several trackers.

The short-term recommendation is to specify and use **(1) with (2) as a fallback**, and to treat **(3)** as the long-term architecture for inter-clearinghouse settlement.

## Current constraints

- A reserve is bound to exactly one tracker via `R6`, which stores the tracker NFT ID.
- During redemption `contract/basis.es` checks `trackerNftId == expectedTrackerId` where `trackerNftId` comes from the tracker data-input box and `expectedTrackerId` is the reserve's `R6` value.
- A tracker box commits its ledger in `R5` as:
  ```
  hash(issuer_pubkey || recipient_pubkey) -> totalDebt
  ```
- A reserve box commits redeemed amounts in its own `R5` as:
  ```
  hash(owner_pubkey || receiver_pubkey) -> (timestamp, cumulativeRedeemedAmount)
  ```
- Normal redemption needs the issuer's signature **and** the tracker's signature on the 48-byte message:
  ```
  key = blake2b256(ownerKey || receiverKey)
  message = key || longToByteArray(totalDebt) || longToByteArray(timestamp)
  ```
- Emergency redemption bypasses the tracker signature after the tracker box is older than `3 * 720` blocks (~3 days).
- The reserve contract enforces `timestamp > storedTimestamp`, so notes used for redemption must be newer than any previously redeemed note for the same `(owner, receiver)` pair.
- Acceptance policies and redemption ordering are evaluated from the tracker's local note state; collateralization uses the issuer's gross debt on that tracker.

Because of the `R6` binding, a note tracked by `T2` cannot be redeemed against a reserve created for `T1`. Cross-tracker movement therefore cannot be a single on-chain redemption against the original reserve; either the movement happens off-chain or the contract must be generalized.

---

## Option 1 — Off-chain cross-tracker novation

### Idea

Move the debt by updating the ledgers of both trackers:

- On the **source tracker** `T1`, reduce `A -> B` debt by amount `X`.
- On the **target tracker** `T2`, create or increase `A -> C` debt by amount `X`.

Both updates are off-chain; no Ergo transaction is required.

### Required signatures

The issuer `A` must sign the new notes:

- A note to `B` for the remaining amount (or a zero-amount cancellation record with a fresh timestamp).
- A note to `C` for the transferred amount.

Each note uses the standard 48-byte signing message with the tracker-specific recipient key. The source tracker signs the updated `A -> B` record; the target tracker signs the new `A -> C` record.

### Atomicity problem

The two tracker updates are independent. If `T1` reduces `A -> B` but `T2` fails to create `A -> C`, the debt simply disappears from `T1` without appearing on `T2`. Several mitigations exist:

- **Coordinated two-phase commit**: both trackers prepare the updates, exchange signed promises, and commit only after both promises are valid. If one tracker does not respond within a timeout, both abort.
- **Trusted broker**: a party that has accounts on both trackers holds both signed updates and releases them to the trackers only when both confirm.
- **On-chain fallback**: if off-chain coordination fails, fall back to Option 2.

### Timestamp and replay rules

- The new `A -> B` note on `T1` must have a timestamp greater than the current `A -> B` record on `T1`.
- The new `A -> C` note on `T2` must have a timestamp greater than any existing `A -> C` record on `T2`.
- If the transfer is later partially redeemed on `T2`, the reserve tree on `T2`'s reserve will store that timestamp; any later `T2` note must use a greater timestamp.

### Trust model

- `A` must consent (signs both notes).
- `T1` and `T2` must each be honest in signing only valid state transitions.
- There is no on-chain enforcement that the two ledgers stay consistent; misbehaviour is detectable but not automatically slashable.

### Acceptance-policy impact

- `B`'s acceptance policy on `T1` becomes irrelevant once the debt is reduced.
- `C`'s acceptance policy on `T2` must accept the new `A -> C` note.
- Collateralization checks on `T2` will only see `A`'s liabilities on `T2`. If `A` still has liabilities on `T1`, `T2`'s collateralization check will undercount `A`'s total debt unless the trackers share liability snapshots.

### When this works

- Both trackers are online and trusted by the parties.
- The parties want to avoid on-chain fees.
- No immediate redemption is required (the debt continues to circulate).

---

## Option 2 — On-chain redemption + re-issuance

### Idea

Settle the source debt on-chain, then recreate the obligation on the target tracker.

1. `B` redeems the `A -> B` note from `A`'s reserve backed by `T1`.
2. After the on-chain redemption confirms, `B` (or a broker) holds the collateral.
3. `A` signs a new note `A -> C` on `T2` for amount `X`; `C` accepts it.

### Redemption step

- Uses the normal redemption flow on `T1`:
  - `A`'s signature from the original IOU note.
  - `T1`'s tracker signature (or emergency redemption after ~3 days).
  - AVL proofs against `T1`'s tracker tree and the reserve's redemption tree.
- The reserve must have enough collateral and must not be in a state that blocks redemption (e.g., distressed-reserve FIFO ordering on `T1`).

### Re-issuance step

- `A` creates a new note on `T2` with recipient `C` and amount `X`.
- `T2` signs the note and updates its ledger.
- The new note is backed by whatever collateral `A` has on `T2` (which may be zero if `A` is trusted via a whitelist).

### Costs and timing

- One Ergo redemption transaction (miner fee + tracker update fee).
- On-chain confirmation time.
- The target tracker then commits its new state on-chain in its regular update cycle.

### When this works

- The source reserve is sufficiently collateralized and the source tracker is cooperative (or the emergency window has passed).
- `C` is willing to accept `A`'s credit on `T2`.
- The parties can afford the on-chain fees.

### Relation to Option 1

Option 2 is the fallback when the source tracker is uncooperative, when the target tracker does not trust the source tracker, or when atomicity of the off-chain transfer cannot be guaranteed. It is also the natural way to convert a `T1` debt into a `T2` debt when `B` and `C` are different parties and `C` wants immediate collateral backing.

---

## Option 3 — Multi-tracker reserve gateway (contract extension)

### Idea

Allow one reserve to recognize multiple trackers, so notes from any recognized tracker can be redeemed against the same reserve. This turns the reserve into a gateway between tracker networks.

### Required contract changes

In `contract/basis.es` (and `contract/basis-token.es`):

- Change `R6` from a single tracker NFT ID to a commitment to a set of tracker NFT IDs. Options:
  - An AVL tree of tracker NFT IDs.
  - A fixed-size collection if the number of trackers is small.
- Replace the strict equality check:
  ```ergoscript
  val trackerIdCorrect = trackerNftId == expectedTrackerId
  ```
  with a membership check:
  ```ergoscript
  val trackerIdCorrect = recognizedTrackers.contains(trackerNftId, membershipProof)
  ```
- Preserve the multi-tracker set across all actions (redemption, top-up, refund initiation, refund completion).

### State implications

- The reserve's redemption tree remains keyed by `(owner, receiver)` and is independent of which tracker issued the note.
- This prevents double-redemption across trackers automatically, because the reserve tracks cumulative redeemed amounts globally.
- Timestamp monotonicity still applies per `(owner, receiver)` pair on the reserve, so all trackers must coordinate on timestamp ordering.

### Tracker coordination

To move a debt from `T1` to `T2` under a multi-tracker reserve:

1. `T1` signs a note that reduces `A -> B` (or marks it as transferred out).
2. `T2` signs a note that creates/increases `A -> C`.
3. Both trackers update their own AVL trees and commit on-chain.
4. Redemption can then be initiated through either tracker, as long as the reserve recognizes both.

### Acceptance-policy impact

- Collateralization predicates must aggregate the issuer's gross debt across **all** recognized trackers. A tracker that only sees its own ledger will undercount liabilities.
- This requires either a federation protocol that shares liability snapshots or an on-chain commitment to the combined liability set.

### When this works

- Long-term inter-clearinghouse settlement.
- Gateways between regional/community trackers.
- Higher implementation cost: new contract, new proof logic, new tracker federation rules.

---

## Comparison

| Criterion | Option 1: off-chain novation | Option 2: redemption + re-issuance | Option 3: multi-tracker reserve |
|---|---|---|---|
| On-chain transaction | None | One redemption (+ target tracker commit) | None for the transfer itself; tracker commits continue as usual |
| Contract changes | None | None | Yes: generalize `R6` |
| Tracker cooperation required | Both trackers must cooperate | Only source tracker for redemption (or emergency) | Both trackers must be recognized by reserve |
| Trust assumptions | Trust both trackers / broker | Trustless on-chain settlement for source debt | Trust federation / multi-tracker set management |
| Double-spend protection | Coordination / social | Contract-enforced | Contract-enforced globally |
| Collateralization accuracy | Undercounts cross-tracker debt | Correct after settlement | Requires cross-tracker liability aggregation |
| Implementation effort | Low (protocol only) | Low (uses existing flows) | High (contract + tracker federation) |

---

## Recommendation

1. **Immediate path**: specify and implement **Option 1** with a documented two-phase atomicity protocol and a trusted-broker pattern. Keep **Option 2** as the standard fallback.
2. **Long-term path**: design **Option 3** for production inter-clearinghouse use, including a federation protocol for sharing issuer liabilities across recognized trackers.

This matches the existing architecture: within a single tracker, debt already moves off-chain via novation (`specs/spec.md`, `contract/basis.es` comments). Extending that off-chain model across trackers is the smallest conceptual leap, while on-chain redemption remains the trust-minimized backstop.

## Open questions

- Should a cross-tracker transfer carry an explicit signed "transfer receipt" from `A` that binds both the source and target notes? This would improve auditability but is not required by the contracts.
- How should a tracker discover the set of trackers recognized by a multi-tracker reserve in Option 3?
- Should acceptance policies be extended with a cross-tracker liability oracle, or should multi-tracker reserves be restricted to federations that share full state?

## References

- `contract/basis.es` — reserve contract and redemption logic.
- `contract/basis-token.es` — token-backed variant with identical tracker binding.
- `specs/spec.md` — debt transfer (novation) within one tracker.
- `specs/basis_protocol.md` — future extensions including multi-tracker reserves and tracker federation.
- `specs/informal_clearing_systems.md` — cross-tracker gateway discussion.
- `specs/server/redemption_state_spec.md` — redemption flow, AVL proofs, and emergency redemption.
- `specs/server/tracker_box_update_spec.md` — tracker box commitments and R5 format.
- `specs/server/r6_register_implementation_spec.md` — current single-tracker binding via R6.
- `specs/redemption_acceptance_policy.md` — redemption ordering and policy checks.
