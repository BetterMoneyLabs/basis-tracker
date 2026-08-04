# basis-tracker

Tracker for Basis offchain notes. Foundational tool for monetary democratic federalism (as any community can run the 
tracker for its own money, having optional common ground with the rest of the world via Ergo blockchain and its assets).

Released under public domain (CC0) license.

## Basis - offchain IOU money for digital economies and real-world communities

Basis is efficient offchain peer-to-peer cash, optionally backed by on-chain reserves but also allowed to be created 
purely on trust, and so creating credit (unbacked IOU money). Its use cases are now thought as follows:

* micropayments, such as payments for content, services, resources usage in p2p and distributed systems. Notable
  difference from Lightning / FediMint / Cashu etc is that here a service can be provided on credit (within certain limits),
  which would boost growth for many services, allow for globally working alternative to free trial, and so on.

* community currencies, which can be about small circles where there is trust to each other, using fully unbacked offchain cash,
  more complex environments using fully or partially backed cash, potentially with tokenized local reserves (such as gold and silver)
  etc. Small circles maybe powered by just mesh networks, with no or very limited access to Internet.

* informal and formal clearing systems

* agentic economic networks

Such use cases would definitely win from simple but secure design, no on-chain fees, and no need to work with blockchain at all before need to back issued cash or redeem cash for blockchain asssets.

But there can be more use cases discovered with time!

## Community

We have chats over:
* Telegram: [https://t.me/chaincashtalks](https://t.me/chaincashtalks)

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
* IOU note from A to B represents cumulative debt with format: cumulative debt amount tracked by tracker, where the tracker
  stores `hash(A_pubkey || B_pubkey) -> totalDebt` mappings. The signature from A (sig_A) is computed over
  `key || totalDebt || timestamp` where `key = blake2b256(ownerKey || receiverKey)`. Only one updateable note is stored by a tracker
  per (A,B) pair, and is redeemable onchain. The tracker commits on-chain to the data by storing an AVL tree root digest
  in register R5, where the tree stores `hash(A || B) -> totalDebt` mappings.

* If A has on-chain reserve, B may redeem from A->B note by providing proof of totalDebt from tracker's AVL tree. The reserve
  contract UTXO stores an AVL tree in R5 tracking `hash(ownerKey || receiverKey) -> cumulativeRedeemedAmount`. Redemption
  requires both reserve owner's signature AND tracker's signature on `key || totalDebt || timestamp`. Emergency redemption is available
  after 3 days (3*720 blocks) from tracker creation height if tracker becomes unavailable. After on-chain redemption, the
  reserve's AVL tree is updated with the new cumulative redeemed amount, preventing double redemption.

## Basis Contract

A basic contract corresponding to the design outlined in the previous section, is available @ [basis.es](contract/basis.es).


## Basis Server

The Basis Server (tracker) is the offchain service that maintains the global ledger of IOU notes.
It stores `hash(issuer || recipient) -> totalDebt` mappings in an AVL+ tree, periodically commits
the tree root digest to the Ergo blockchain in a tracker box, and exposes an HTTP API (documented
in `openapi.yaml`) for wallets and agents to create notes, query balances, request redemption
proofs, and monitor reserve status. Anyone can run a tracker; honest behavior is incentivized by
user trust and verifiable on-chain commitments.

## TUI Wallet

`basis-ui` is a terminal wallet built on top of `basis_cli_lib`. It provides a keyboard-driven
interface for managing accounts, issuing and receiving IOU notes, creating and monitoring
Ergo-backed reserves, redeeming notes, and configuring an acceptance policy. The main menu shows
at-a-glance wallet stats (assets, liabilities, net position, and reserve coverage) with
warnings when liability coverage drops below 150%, 120%, or 100%.

## MCP Server

`basis-mcp` is a Model Context Protocol server that exposes the wallet over stdio, allowing
MCP clients (such as Kimi CLI or Claude Desktop) to create accounts, issue and redeem notes,
check reserve status, and manage acceptance policy without shelling out. It wraps the same
typed command cores as `basis-cli --json`; private keys stay in-process and are never echoed
back to the client.

## Future Extensions

* Federated trackers

Instead of a single tracker, we may have federation, like done in Oracle Pools, or double layered federation like done
in Rosen bridge.

* Tracking sidechains

As a continuation of federation tracker idea, we may have tracking sidechains, for example, merged-mined sidechains, to reduce multisig security to majority-of-Ergo-hashrate-following-sidechain security.

* Programmable cash

We may store redeeming condition script hash instead of recipient pubkey just in IOU notes, and add the condition to
other redeeming conditions in onchain redemption action.

* Multi-tracker reserve

Possible to have reserve contract with support for multiple reserves, put under AVL+ tree or just in collection if there are few of them.

For most reserves that does not make sense probably, but multi-tracker reserves can be used as gateways between
different trackers, to rebalance liquidity etc.

* On-chain Privacy

Not hard to do mandatory redemptions to stealth addresses.


## Continuous Integration

This project uses GitHub Actions for continuous integration. On every commit to main/master branches and on every pull request, the following checks are run:

- ✅ **Cargo build** - Compiles all crates
- ✅ **Cargo test** - Runs all test suites  
- ✅ **Cargo clippy** - Lints code for best practices
- ✅ **Cargo fmt** - Checks code formatting
- ✅ **Example execution** - Runs all demonstration examples
- ✅ **Module-specific tests** - Runs tests for specific modules

See [.github/workflows/test.yml](.github/workflows/test.yml) for the complete workflow.

## Implementation Roadmap

The following implementation plan is targeting catching micropayments in P2P networks, agentic networks, etc ASAP and then
develop tools for community trading:

* Do Celaut payment module, where peers can set credit limits and pay each other. Add support for agentic layer, so AI agents can buy computations over Celaut, then requests to other APIs as well.
* Do showcase for agent-to-agent payments

and so on

## References
