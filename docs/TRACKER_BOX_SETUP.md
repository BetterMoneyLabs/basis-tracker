# Tracker Box Setup

The tracker box publisher maintains the on-chain R5 commitment for one
configured tracker NFT. The server does not mint the NFT or create the initial
tracker box. Provision those assets through a separately reviewed wallet flow,
then configure the existing generation before enabling publication.

## Required identity

The active tracker input must have all of the following properties:

- the configured tracker NFT is the first token, with amount one, and occurs
  exactly once;
- R4 is a `GroupElement` equal to the configured tracker public key;
- the configured 32-byte secret derives that same public key;
- R5 is the current serialized AVL commitment;
- its value, ErgoTree, token order, and any R6-R9 registers can be preserved in
  the successor.

The tracker input is a contract box. Its ErgoTree is not required to be the
tracker key's P2PK tree. Fee inputs are different: every fee input must be
token-free and protected by exactly that derived P2PK tree.

## Configuration

Use the TOML configuration shape from `config/basis.toml.example`:

```toml
[ergo]
tracker_nft_id = "<64 lowercase or uppercase hex characters>"
allow_fresh_tracker_generation = false
tracker_public_key = "<66 hex characters or a P2PK address>"
tracker_secret_key = "<64 hex characters>"

[ergo.node]
node_url = "http://127.0.0.1:9053"
api_key = "<node API key>"

[transaction]
fee = 1000000
```

Keep `allow_fresh_tracker_generation = false` for every existing state
directory and every restart. Set it to `true` only for the one intentional
initialization of a new, empty state directory whose configured NFT and initial
root are being bound for the first time; return it to `false` immediately
afterward. It does not authorize minting, replacing, or silently adopting an
on-chain generation.

Do not commit either secret. Configuration `Debug` output redacts the node API
key and tracker secret.

`transaction.change_address` may still exist for unrelated compatibility
surfaces, but the tracker publisher ignores it. Publisher change is always
sent to the P2PK ErgoTree derived from `tracker_secret_key` after that secret is
matched to `tracker_public_key` and tracker R4.

## Signing and submission boundary

For every update the publisher:

1. obtains the tracker and candidate fee boxes as node JSON;
2. obtains each same box from `/utxo/byIdBinary/{boxId}` and Sigma-parses the
   canonical bytes;
3. requires exact JSON/raw equality for ID, value, ErgoTree, ordered assets,
   R4-R9 key set and bytes, and creation height;
4. fetches exactly 10 newest-first, parent-linked headers and requires
   `/info.fullHeight`, `/info.bestFullHeaderId`, and the nested current
   parameter set to describe that same tip;
5. constructs a typed transaction with checked sums and current dust limits;
6. signs in process with ergo-lib `Wallet` and validates the signed transaction
   against the same ordered, duplicate-free exact inputs and state context;
7. sends only the signed transaction to `POST /transactions`.

The publisher does not call `/wallet/transaction/sign`, does not serialize a
`secrets` or `inputsRaw` signing bundle, and does not send the tracker secret to
the node. The node wallet endpoint is used only to discover candidate unspent
fee boxes; each candidate is independently rebound to its exact Sigma bytes and
owner tree before signing.

## Fail-closed conditions

Publication is refused when any of these conditions holds:

- the tracker box is absent or its NFT/R4 authority is wrong;
- the secret is absent, invalid, or does not derive the configured public key;
- node JSON differs from canonical box bytes;
- a fee input has a token or a different ErgoTree;
- input cardinality, order, or uniqueness differs between construction and
  signing;
- the 10-header chain or `/info` pin is incomplete or inconsistent;
- value arithmetic overflows, fee funds are insufficient, or an output is
  dust;
- local proof generation or post-sign transaction validation fails.

Successful node admission is not confirmation or settlement evidence. The
confirmed-chain reconciler owns that later transition and its reorg policy.
