# Basis - offchain IOU money for digital economies and communities

In this writing, we propose Basis, efficient offchain cash system, backed by on-chain reserves but also allowing for 
creating credit (unbacked IOU money). Its use cases are now thought as follows:

* micropayments, such as payments for content, services, resources usage in p2p and distributed systems. Notable 
difference from Lightning / FediMint / Cashu etc is that here a service can be provided on credit (within certain limits),
which would boost growth for many services, allow for globally working alternative to free trial, and so on. 

* community currencies, which can be about small circles where there is trust to each other, using fully unbacked offchain cash,
 more complex environments using fully or partially backed cash, potentially with tokenized local reserves (such as gold and silver) 
 etc

Such use cases would definitely win from simple but secure design, no on-chain fees, and no need to work with blockchain 
at all before need to back issued cash or redeem cash for blockchain asssets. 

But there can be more use cases discovered with time!

## Basis Design

As we have offchain cash with possibility to create credit (unbacked money), we have need to track all the money in form 
of IOU (I Owe You) notes issued by an issuer, for all the issuers. In comparison with fully on-chain ChainCash design,
we have to deal with some security relaxation in the case of offchain notes.

As a simple but pretty secure solution, the following design is proposed, which can then be improved in many directions 
(see "Future Extensions" section):

* every participant has a public key over elliptic curve supported by Ergo blockchain (Secp256k1, the same curve is used
 in Bitcoin)
* only reserves are on-chain. A reserve can be created at any time. A reserve is bound to public key of its owner. 
Anyone (presumably, owner in most cases) can top the reserve up.
* for keeping offchain cash ledgers, we have trackers. Anyone can launch a tracker service (just running open-source
 software on top of powerful enough hardware is needed for that). With time a tracker is getting trust and userbase if 
 behaves honestly. The design is trying to minimize trust in tracker. For example, a tracker cant redeem IOU notes made 
 to other parties, as they are signed, and the signature is check in redemption on-chain contract. If tracker is 
 disappearing, after some period last tracker state snapshot committed on-chain becomes redeemable without it. If tracker
  is starting censoring notes associated with a public key, by not including them into on-chain update, it is still
  possible to redeem them. There could be different improvements to the tracker design, see "Future Extensions" section.
* IOU note from A to B is represented as (B_pubkey, amount, timestamp, sig_A) record, where amount is the **total** amount of
 A's debt before B, timestamp is the timestamp of latest payment from A to B (in milliseconds since Unix epoch),
 and sig_A is a signature for (B_pubkey, amount, timestamp). Only one updateable note is stored by a tracker,
 and redeemable onchain. Thus a tracker is storing (amount, timestamp) pairs for all A->B debt relationships.
 The tracker commits on-chain to the data by storing a digest of a tree where hash(A ++ B) acts as a key,
 and (amount, timestamp) acts as a value.

* If A has on-chain reserve, B may redeem offchain from A->B note, by providing proof of (amount, timestamp).
 Reserve contract UTXO is storing tree of hash(ownerKey||receiverKey) -> (timestamp, cumulativeRedeemedAmount) pairs.
 The value format is: timestamp (8 bytes big-endian) ++ cumulativeRedeemedAmount (8 bytes big-endian) = 16 bytes total.
 During redemption, the contract verifies that the note's timestamp is **greater than or equal to** the stored
 timestamp, which prevents replay attacks with old notes while still allowing the remainder of a partially
 redeemed note to be redeemed later. After on-chain redemption, A and B should contact offchain tracker
 to update their records before next payment from A to B is done.

 Redemption additionally enforces, in both contract variants:
 - **Replay protection without a lookup proof:** context variable #7 (AVL+ lookup proof for the note
   key in the reserve's redemption tree) is provided from the second redemption on. For a first
   redemption it may be omitted, in which case the contract uses `AvlTree.insert`, which fails if the
   key is already present — so replaying a redeemed note without #7 is rejected.
 - **Single reserve input:** exactly one input with the reserve contract and the owner's public key in R4
   may appear in a redemption transaction, so two reserves of the same owner cannot be mixed to redeem
   the same note twice.
 - **Token reserves preserve ERG:** in `basis-token.es` the reserve output must keep at least the input's
   ERG value in both redemption and top-up actions, so the reserve cannot be drained of its ERG (storage
   rent) while tokens are redeemed.

 A reserve only counts as collateral once the tracker has validated its setup when first seen on-chain
 (box creation is not a spend, so the contract cannot check it): R5 must hold the empty AVL+ tree
 (flags `0x03`, 32-byte keys), R7 must be absent or `Long 0` (no pre-initiated refund), R6 must reference
 the tracker's own NFT (a singleton token), and R4 must be a valid secp256k1 point. A reserve with a
 pending refund is never counted as collateral.

* Debt Transfer (Triangular Trade): The protocol supports transferring debt between creditors with debtor consent.
 Example: A owes 10 ERG to B. B wants to buy from C for 5 ERG. Instead of on-chain redemption:
  1. B requests A to sign a new note A->C (5 ERG transferred)
  2. A signs the note, tracker signs the note
  3. New note A->C (5 ERG) is created; the A->B note keeps its cumulative total of 10 ERG on the tracker
  4. C can now redeem A->C note from A's reserve
 This enables efficient multi-party settlements without on-chain transactions.

 **WARNING — step 3 is not safe as written, and is currently rejected by the tracker.** Lowering the
 committed cumulative debt for an existing (A, B) pair requires B's consent, not just A's: the contract
 pins every redemption to the tracker tree, so a lowered totalDebt makes all of B's existing notes fail
 `trackerDebtCorrect` permanently (the emergency path requires the same tree match). Worse, B could
 redeem 10 ERG before the change is committed and C could then redeem 5 ERG, so A pays 15; and if B had
 already redeemed 8, "remaining 5" is meaningless against a cumulative redeemedDebt of 8.

 As of PR #14 the tracker therefore **refuses any note that lowers the committed debt** for an existing
 pair (`NoteError::DebtDecreaseNotPermitted`), and `amount_redeemed` carries forward instead of resetting.
 Transferring debt to a *different* creditor is unaffected, because that is a different key in the tree.
 Reviving the flow above requires a creditor co-signature endpoint that does not exist yet; until then a
 debt transfer must be settled as redemption plus re-issuance. See specs/pr14_triage.md (S3).

## Basis Contract

A basic contract corresponding to the design outlined in the previous section, is available @ [basis.es](../contract/basis.es).

**Reserve Owner Refund (Exit):** The reserve owner can unilaterally exit without tracker or creditor
cooperation, protecting the owner from censorship. To protect creditors from the owner silently draining
collateral, refund is two-phase:

1. **Initiate refund (action #2):** the owner signs a transaction setting register R7 to the initiation
   height (current or slightly future-dated, to tolerate delayed block inclusion; backdating is rejected).
   One-shot only: re-initiation is not allowed.
2. **Complete refund (action #3):** after a waiting period of 43200 blocks (~2 months), the owner signs a
   transaction spending the reserve box and taking all funds and tokens.

Redemptions and top-ups remain fully enabled during and after the waiting period (both preserve R7), so
creditors have ~2 months to redeem their notes before the owner can withdraw. Redemption is also not
disabled after the deadline, so an owner who initiated but never completed the refund does not freeze the
reserve. Only full withdrawal is supported: a partial refund would require tracker attestation of
outstanding debt, reintroducing the censorship vector the refund protects against.

Wallets and trackers should monitor reserves for R7 being set and notify creditors; acceptance predicates
should reject new notes backed by reserves with a pending refund.

**Compiled Contract Address (P2S):**
```
2XoR96YVZfWybxeo7qdMkA1BRhxFivm8gTuLLuKREiW6833XWK4mA43DrJMDCvLqB9AwyduGgHBnGmmg2jrkUZAunRDu7t8718EkwiJTvp9rtEWEXMMJeMStjQB5BeuxsUc9ujW3aCMvxdS8VeYyZ14B8sSGu7CwJFmeaZDHiftsT3tUBqBfgau4wZTvhEtYEsFsqvpwsjcPAe4aGhFzJNSPhKqdSBrFKee3Dg4qFmGhb8Wu6DsdmhTrW6jnucnf17PDi2rQ9pgJvKrfs2KPbeZf9gV89vEga4kZk4u3bbSFTVk3s9EhuoUmfyhvC3ZaNv9DSPtzCcjvnL7x6smyLigsgb4QSEDnzX4HjJg5CfA6mh4Jn9hoL1412EVnY2xb8cYz76VEgU8J12n4DHrVsV9NzYAHxe6ogVNptUbAgykiru81pz3aZ6LfYySJt6ESVVGxm6i49ScGjawqsKCxjykCndh3MNQnRajZMgQMH2qFZhbd9KXCnLQkS6NAjtNfMVcx1KykdiSrLVTu3nphUZhT9MjhfxtZcojSZoQmGAy95zhAJpDMgUXBfuCSoUFUzPAuSMv2pBuEXiUR3wDQC7pDFYUDy68QCtTUTDtn9cYyNgc8EZWQSB76wsKhCNod1Q9RV85piLF5PJDMBXh4M8gjEzsq244xDdW6t2f4W4A2ZVDNw2gdSU3FHYxfB45c5cxkyR5qrXyoJT56c2STfFm3vK1ZfXzvckq48uAE1QRGpM9xALiXtdTx1iQaQjy2LkaURha6ewN5NDufyqntwBPk72NLadL8BDtLyReRtUWqbFZo94h4prjHVT25ABHbAqiJuTCHvmfTqi2V5gDwRW7oLk4gvBwswxJVvKmuKSG6sH5HAzQGoF6Kwcwdvq8Lbv5RbuAnm3N7hLnS1rBUWKXfaJCJjptMkeKxzMPhjfjnZTkMXwW3jSBxhpLeuWxrBrfjTFaVR93erQFVTvF9C1FsULgqUPjzPmcHn5Drp4cp4Y2pjv793k6KdykbugL6M6wt75hmKG5jsmrfsSL6AN13j3
```

## Basis-Token Contract (Token-Based Reserve)

A variant of the Basis contract that uses custom tokens instead of ERG for reserve backing is available @ [basis-token.es](../contract/basis-token.es).
It supports the same four actions and the same two-phase reserve owner refund as the ERG-backed Basis contract.

### Token Structure

A `basis-token.es` reserve box holds exactly two tokens:

- **Token #0**: Reserve NFT (amount 1). Identifies the reserve and must be preserved in every output.
- **Token #1**: Reserve token. This is the custom token used as collateral. Its amount is reduced during redemption and increased during top-up.

### Redemption

Redemption follows the same signature and proof requirements as the ERG-backed contract, but the redeemed amount is measured as the decrease in token #1 amount:

```
redeemed = SELF.tokens(1)._2 - selfOut.tokens(1)._2
```

The recipient output receives the redeemed amount as reserve tokens, while the reserve output's ERG value remains unchanged (only storage rent needs to be preserved).

### Top-up

Top-up increases the reserve token amount by at least 1 whole token unit:

```
selfOut.tokens(1)._2 - SELF.tokens(1)._2 >= 1
```

Unlike the ERG-backed contract, there is no 0.1 ERG minimum because custom tokens are indivisible.

### Token Preservation Rules

- The output reserve box must contain exactly two tokens.
- Token IDs at positions #0 and #1 must match the input reserve box.
- Token #0 (reserve NFT) amount is preserved by the contract logic (Ergo does not allow 0-value tokens).
- Token #1 (reserve token) amount may change according to redemption or top-up logic.

### Refund

On completion of the two-phase refund, the owner takes **all** ERG and **all** tokens (reserve NFT and reserve tokens), destroying the reserve box.

**Compiled Contract Address (P2S):**
```
EUNip3sGUdzr5Leg8rRwRMyZGUis42syLiBYwsj5JbmrVNAh41LUzNFnFTEzpEvf3jMUzJdyfkJi6aVjbs6NLskmHiuCPYwcRFjdqfBwc9NXUnf4hv3heFRDnzXDWK92KAz8JpXSx9Rnd2b4HnkBq8HzbnfSRpRVQqRJZqQaVoceZqcT87sBTNii4Hu6zUPrGaQwsR3QFnNjoKsDrALMTELk8eUcxHhyWbJPPcSYWdf5nFTskqt5F2yZ5FBsKfkCsxskM31FKXRXt7RAWrJWA6QTUzrQT2kk4a8G1G7AYrXRFc2kXHGTH9bjXaNvM4JRu1CkDb2hqLzt5jouzT1UdZkCHqG9LwWa7ULSDfzvdgybQhBv8UC9LnHegQ7FEYyqHhL4ZZpsqr8cGbznE1QCWWbxcGLKWoDhCCt3j6Bv36AoaTPRcffevReEWMk7FTE6s2n391CLV6W8G2HS2zkStnmAKDmdardZJJ2tb3zQPrqM5Y2umufxDV9VCj6t18XBPSNDF8YtPXfwkPLUAyfeEE8JM4wCPNQ33vvH8irVphUagT92U5jLp9B8Z54ceJrLFodaj74kxECULy9ufmC6jGDNbeGj6AM6pxV6HBPgYpBNw8Qupc35se4YW7odPjErov8pZuVHnGusNu4huP1LFm2VL4aqMnLpriHTvx61DG7SCEUxZhEjWtnS5xxnp67fAzibCckBsKZeFsqEGro8pJnS1ZiPAwggz2dLorMoK285Rx8QZNLiRzfULwnWzSGgtoqKZCkKc7QJXiEzivAe4FHC32grsU2KvAb9sv79ardmpgpJTmbNLRAWiDwBXbTLGbF5AJkCcfPCvEoiaLCRERVYpJRZ4XZHo5A14eCFgwvUhDtPpZeJJXqNgGmsbaXZGPXKwdzaE8mA9uZ3ANcZ3KrdYL5HV7w7wrNNMWNzvBSfzM3uC5LiZmaxuWNph9YFLXHEbEfbhjXvGeAMZTGKntErw7NobRPQB6yCVeGMUk5HCDxJxkHAbn9oLxC2Nv3Wpp4cVMf6ixeTS5qL7he36pqkNPAcspPGHFJEajbs4fkaQd9wTgtbGCa7CXAWaP5s1S5pWCBSjnYkgxhjRYa6CwCnnV1u9ksA3Gb3PBr6Z8T2wicYqW925b3wxzNJKs5qotwCDzYthZjiGHGdtFA1vg32vKyGbeoPCXMgmn9uh1dcmnu2m7GVBEjQZ7LYHvuPZp34ykfTTFA
```

## Offchain Logic

### Tracker

Tracker is publishing following events via NOSTR protocol as relay:

* note - new or updated note, along with proof of tracker state transformation and digest after operation
* redemption - redemption done from a reserve
* reserve top-up
* commitment - posting data for on-chain tracker state commitment update (header, proof of UTXO against header, UTXO with commitment)
* 80% alert - tracker is posting it when debt level of some pubkey reaching 80% of collateral 
* 100% alert - tracker is posting it when debt level of some pubkey reaching 100% of collateral

Then it also supports following API requests which can be run separately from relay potentially:

* getNotesForKey - returns all the notes sssociated with a pubkey
* getProof - get proof for a note against latest digest published by the tracker (not necessarily committed on-chain)
* getKeyStatus - returns current collateralization of a pubkey along with other important information. Useful for light
wallets and clients which are ready 
* POST noteUpdate - create or update a note

## Security Assumptions

We assume that tracker is honestly collecting and announcing notes it has. However, malicious trackers may deviate from
honest behaviour.

Tracker can simply go offline, but then the latest state committed on-chain is still redeemable,

Tracker may remove debt notes of protocol participants. This problem can be tackled with the anti-censorship protection
from "Future Extensions" section.

**This is not implemented.** In the current contracts every redemption path, the emergency path included, requires
`trackerTree[key] == totalDebt`, so a tracker that omits a note from its committed tree -- or lowers its value --
blocks that note permanently. The 3-day emergency branch is not a remedy: it relaxes only the tracker's *signature*
requirement, not the tree match, and it only becomes available once the tracker box is more than 3 days old.
A tracker that recreates its box regularly therefore never triggers it. README.md was corrected to say so;
see specs/pr14_triage.md for the full trust-model notes.

Tracker may collude with a reserve holder to inject a note with fake timestamp in the past to redeem immediately. 
Tracker would be caught in this case. For making this case impossible with contract, technique similar to anti-censorship 
protection can be used.

Known limitation: the note signing message (`key || totalDebt || timestamp`) is not bound to a specific reserve
contract or collateral type. If the same owner operates multiple reserves (e.g. an ERG reserve and a token
reserve), a note signed once could in principle be redeemed against either of them. Binding the reserve into
the signed message is a deferred protocol change (it alters the message format on both chains of the
implementation).

## Wallet

## Future Extensions

* Anti-Collusion Protection

Let's suppose that, at time t1, we have:

(Bob -> Alice, 2, 9), with the 9th note signed by Bob.

And, at time t2, we have:

(Bob -> Alice, 3, 10), with the 10th note signed by Bob.

Bob (at least, he incentivized to) informing tracker, and the tracker commits on-chain 
the latest nonce seen. Also, tracker's signature is required for normal redemption.

So at the moment t2:

1) if committed state is (Bob -> Alice, 3, 10) , Alice can't withdraw (Bob -> Alice, 2, 9)
2) if committed state is (Bob -> Alice, 3, 9) , Alice can withdraw by colluding with the tracker , 
    and the misbehavior has onchain footprint

Possible to introduce protection from the collusion by making debt amount ever increasing (so then it is amount of 
offchain debt of Bob before Alice, including redeemed), and  storing redeemed amount in Bob's reserve contract as well.

* Anti-Censorship Protection

If tracker is starting censoring notes associated with a public key, by not including them into on-chain update, it is still
possible to redeem them with anti-censorship protection. For that, tracker box should be protected with a contract which
has condition to include spent tracker input's id into a tree stored in a register. Then tracker is storing commitment to
all it previous states, basically, and we can use that to add a condition to the reserve contract to allow redemption of 
a note which was tracked before but not tracked now, and also not withdrawn. 

* Federated trackers

Instead of a single tracker, we may have federation, like done in Oracle Pools, or double layered federation like done
in Rosen bridge.

* Tracking sidechains

As a continuation of federation tracker idea, we may have tracking sidechains, for example, merged-mined sidechains, to
reduce multisig security to majority-of-Ergo-hashrate-following-sidechain security.

* Programmable cash

We may store redeeming condition script hash instead of recipient pubkey just in IOU notes, and add the condition to 
other redeeming conditions in onchain redemption action.

* Multi-tracker reserve

Possible to have reserve contract with support for multiple reserves, put under AVL+ tree or just in collection if there
 are few of them.

For most reserves that does not make sense probably, but multi-tracker reserves can be used as gateways between 
different trackers, to rebalance liquidity etc. 

* Privacy 

Not hard to do redemptions to stealth addresses. 

## Economy

## Implementation Status

The following milestones have been reached as part of migrating the Basis implementation
from the ChainCash repository:

* **Basis reserve contract tests** — Scala contract tests (`BasisSpec`, `BasisTokenSpec`) are in
  `scala/src/test/scala/basis/contracts/` and run with `sbt test`.
* **Token-based reserve contract** — `contract/basis-token.es` provides a custom-token-backed variant.
* **Schnorr signing / verification** — Canonical 80-byte signing message (binding each signature to the
  tracker's NFT id) and 65-byte Schnorr signatures
  are implemented in both Rust (`crates/basis_offchain/src/schnorr.rs`, `crates/basis_core/src/impls.rs`)
  and Scala (`scala/src/main/scala/basis/offchain/SigUtils.scala`), with shared test vectors in
  `specs/SCHNORR_SIGNATURE_SPEC.md`.
* **Tracker service** — Rust crates (`basis_server`, `basis_store`, `basis_trees`) implement off-chain
  note tracking, reserve scanning, AVL+ tree state commitment, redemption building, and the HTTP API.
* **Reference clients** — `basis_cli` (CLI / REPL / `--json`), `basis_app` (TUI), and `basis_mcp`
  (MCP server for agents) are available under `crates/`.
* **Documentation** — Protocol specs, whitepaper, and contract docs are in `specs/` and `docs/basis/`.

## Future Extensions

* Celaut payment module, where peers can set credit limits and pay each other.
* Showcase for agent-to-agent payments.
* Wallet for community trading (e.g. Telegram bots).
* Alternative for NOSTR zaps.

## References
