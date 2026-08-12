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
 During redemption, the contract verifies that the note's timestamp is **greater than** the stored timestamp,
 which prevents replay attacks with old notes. After on-chain redemption, A and B should contact offchain tracker
 to update their records before next payment from A to B is done.

* Debt Transfer (Triangular Trade): The protocol supports transferring debt between creditors with debtor consent.
 Example: A owes 10 ERG to B. B wants to buy from C for 5 ERG. Instead of on-chain redemption:
  1. B requests A to sign new notes: A->B (5 ERG remaining), A->C (5 ERG transferred)
  2. A signs both notes, tracker signs both notes
  3. Old note A->B (10 ERG) is cancelled, new notes A->B (5 ERG) and A->C (5 ERG) are created
  4. C can now redeem A->C note from A's reserve
 This enables efficient multi-party settlements without on-chain transactions.

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
3PQnJ92Krn6NeM1GdMSmNayw34Nuud7UKMoKSTRUTucsNybh99K1HEfjZqyvP7cPag1yBkDv3ruMAgb2NsVKq3tAygjHz7mKDzHK6CJGhD3WfNViD7DoViqbgsXrzvs6Kt8Wyzb48uGqJAFQFWes6ZPKELqUZowy8xtVCS5w1VwnyaeRiWpEyUVGaEHw3qWo5DcVxzmMAP8XXhVTw1rYYrUxsyGPNaBxQkkkTVD9L3bmw77EfeAJgJ1hLxghykNofHscHtMtES4v5FSfqke3Huun81S7gNoraEnsR6Dy6YnQgrBswwCZhyGc89YeNFQn1TCFh5Hct3nKGrd1bV5zoCw67Q9fKtoaCtvcPQ2GDWycGKNRNgyAnPEa8WbHbTEVcjAN25aBwhnY5LFGqYxnUAjhpfkTPJ4FJWRijSqMESzpyrmhTLZdivmn4YSwcchVZr7bHGbfncEDwqPKefdoxNnVPxuVdmeqQXL3aDL7TaqWgExzz1UPXHw3UiKYTUkNgQKCN4WV3LHqc9PecoisL77ydVbSCxPapaX2zTf26F8bGK3hsTVBZnMkt93SJP5GmPgZU5FT9NkFh4okjXK9ce2wmA4MV93ySyYnUKGwTRFJWwE7G1MYqBqTY3ESkn8PJHqVuL4cgtuV2GEPagKt19befRAuUV3FaLGVPJMzpKdANd7hKGZRcy3DnPfT1Q9dyFD4VpdBgFRXJWaaDqYjL7ni4nJcKKam9P395wRRnjGWhTV4hv3KoxC8Xk2CZAUjhkTzvuNHxQrLsWjyrKWJqZgs2uZxoAEHEobDegYWiTcnFCPU9EeJxZLSjysDFninqpQvA66Yt1SvJnSZm49RKsaoR98UJVScdiQfNZE76zTYBioXGatdRz7QVkXDzDPjPMu9Hhepc2XbHqo3ia8tszHptbnSzm2R3PC7iu2Tnhu3QT
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
96HrjMftJd4NbjzufhMHXyZqzaUbdc5zUqtSySUnyEZoHogM9TD9RsRNkhq83tGaWTH4ZiFeDKzAdHkQyi1SWwMdkKtDaDhobhsrb5tjEDZkYhhF5aPGf3b4fUo3W23tec9N7GyzA6buyhRYzf3DqVWRsJGzCEdYeDfcMYKXon3wMxMFgQPX4FUbCAGe32dCd1aiFwQwx4Rnjy19G6qPaACdZvfSNFLgM1ur3U6HSSMX22g5o8MbbxzNeKScWFedW76z1zCmC38BrgiSv1qC7395yi9y4dDy2y26Tgrc2MPxvned5j1F3cTTTY9HVPYcS9U4vQocDRHufEkMG4dyu7eQqiHbqa6962B1jKQUMofyNV2mehQDrTzfzT5yHPsTeAGMTbDHeDECsmmJ2ideonha9VBuEP6fivixWjej43spbMZDcy3SCNa5gTrLuhh8gN4j8CbMncTXYghrxNdRqPbiUfJy8rMpcdeRaDnbodibGAyJqzzh95ZC6FHPwL7UQZbFbc472WSPtPRXU9g7QpZasdYtZb4GrHqvUJASsLwgYsRevJ4QZ24nrcUXv5ttziaVsGfeCW35viGB5zc3Sxk8e37smwQzWPfAGvsLfgy98dJfmNFyQUCNUyGX5qq4uJrKNaRamouL4S1VNFcgVaJTdGDTT7ykcy6qLmaFg6tUiAsLLC2p4GGB83fqsw13hfmxCxMGuFQUFu6aNc9W31wMMJ2beCN7Xb7cUkffzEXZnjiqFKMicTHsmvLz2mZuAgiKykrxqNTMqmTd2DjGsuYmQNtLXqS4j5d9qXnSVxmeVGp3cqcC7NYF9ujvLT38yqfnk5tZ5X9gnMXgkkKk8MfuUgY5a2ccSgbetkj7yNk1ciHK1QYGrvydDPWB2kJEQb9yAb72Uf1jZXVH1kSAr7vv1BBRQWveJcsjD6GQVKXJrLTGLrR5XV1uYK7uQSR4XYEi5yJsjk4rPMCDtt2FDsrUk7bo9suFWq2sqZxQZZWEgoKkwog6DtvyX4cyMpT45eeqjctBTjnAnJRf8CgBuyEhfU8RMfG7dPNVk9EWS3JbGmwow4R95Ae5P
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

Tracker may collude with a reserve holder to inject a note with fake timestamp in the past to redeem immediately. 
Tracker would be caught in this case. For making this case impossible with contract, technique similar to anti-censorship 
protection can be used.

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
* **Schnorr signing / verification** — Canonical 48-byte signing message and 65-byte Schnorr signatures
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
