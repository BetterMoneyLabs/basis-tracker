# Testing Note Creation and Redemption with Two TUI Wallets

This guide shows how to test Basis IOU notes and redemption using two
`basis-ui` TUI wallets on the same machine. Alice acts as the issuer and Bob
as the recipient/redeemer.

> **Two-wallet isolation:** each TUI wallet stores its config and accounts under
> `$HOME/.basis/`. To run two wallets on one machine, launch each one with a
> different `HOME` directory.

---

## Prerequisites

1. Build the workspace:
   ```bash
   cargo build -p basis_server -p basis_cli -p basis_app
   ```
2. A running tracker server reachable at a known URL (default `http://127.0.0.1:3048`).
3. For **pure-credit notes only**, no Ergo node is required.
4. For **backed notes with redemption**, you also need:
   - An Ergo node connected to the tracker server.
   - A tracker NFT ID and a funded node wallet.
   - See [`TRACKER_BOX_SETUP.md`](TRACKER_BOX_SETUP.md) and
     [`BUILD_AND_CREATE_RESERVE.md`](BUILD_AND_CREATE_RESERVE.md) for setup details.

---

## Option A — Pure-Credit Notes (no reserves, no redemption)

The fastest way to see two TUI wallets exchange notes is the built-in LETS demo:

```bash
./demo/lets_tutorial/run_lets_tutorial.sh --members alice,bob --tmux
```

This starts a tracker and opens Alice's and Bob's wallets in separate tmux
windows. From there:

1. In **Alice's** wallet: `Notes → Create Note` → select `bob` → amount `2000000000` → confirm.
2. In **Bob's** wallet: `Notes → Notes Received` to see the new note.
3. Both wallets: check the main-menu stats for updated assets/liabilities.

See [`demo/lets_tutorial/README.md`](../demo/lets_tutorial/README.md) for more.

---

## Option B — Backed Notes with On-Chain Redemption

This option exercises the full reserve-backed flow: Alice creates a reserve,
issues a note to Bob, and Bob redeems it.

> **Known limitation:** the current TUI signs the issuer's redemption
> co-signature locally, so the redeemer's wallet must have access to the
> issuer's account. For a two-wallet test, import Alice's account into Bob's
> wallet before redeeming, or run the redemption from a wallet that contains
> both accounts.

### 1. Prepare isolated home directories

```bash
export ALICE_HOME=/tmp/basis-alice
export BOB_HOME=/tmp/basis-bob
mkdir -p "$ALICE_HOME" "$BOB_HOME"
```

### 2. Create accounts

```bash
HOME="$ALICE_HOME" ./target/debug/basis_cli account create alice
HOME="$BOB_HOME"   ./target/debug/basis_cli account create bob
```

Record the public keys:

```bash
ALICE_PUBKEY=$(HOME="$ALICE_HOME" ./target/debug/basis_cli account info --json | python3 -c "import sys,json; print(json.load(sys.stdin)['pubkey_hex'])")
BOB_PUBKEY=$(HOME="$BOB_HOME" ./target/debug/basis_cli account info --json | python3 -c "import sys,json; print(json.load(sys.stdin)['pubkey_hex'])")
echo "Alice: $ALICE_PUBKEY"
echo "Bob:   $BOB_PUBKEY"
```

### 3. Write TUI configs

Create `$ALICE_HOME/.basis/ui.toml`:

```toml
server_url = "http://127.0.0.1:3048"
current_account = "alice"

[acceptance]
default = "reject"
root = "trust_bob"

[[acceptance.predicates]]
name = "trust_bob"
type = "whitelist"
holders = ["<BOB_PUBKEY>"]
max_debt = 5000000000

[address_book]
bob = "<BOB_PUBKEY>"
```

Create `$BOB_HOME/.basis/ui.toml`:

```toml
server_url = "http://127.0.0.1:3048"
current_account = "bob"

[acceptance]
default = "reject"
root = "trust_alice"

[[acceptance.predicates]]
name = "trust_alice"
type = "whitelist"
holders = ["<ALICE_PUBKEY>"]
max_debt = 5000000000

[address_book]
alice = "<ALICE_PUBKEY>"
```

Replace `<ALICE_PUBKEY>` and `<BOB_PUBKEY>` with the values from step 2.

### 4. Launch the wallets

Open two terminals and run:

```bash
# Terminal 1 — Alice
HOME="$ALICE_HOME" ./target/debug/basis-ui

# Terminal 2 — Bob
HOME="$BOB_HOME" ./target/debug/basis-ui
```

Both should show `Server: ● connected` in the header.

### 5. Alice creates a reserve

In **Alice's** TUI:

1. Go to `My Reserves`.
2. Select `Create Reserve`.
3. Enter the tracker `NFT ID` and a collateral amount (e.g. `1000000000` for 1 ERG).
4. Confirm and submit to the tracker server.

The server broadcasts the reserve creation transaction using its configured
Ergo node wallet. Wait for the reserve to appear in `My Reserves`.

### 6. Alice creates a note to Bob

In **Alice's** TUI:

1. Go to `Notes → Create Note`.
2. Select `bob` from the address book.
3. Enter an amount, e.g. `500000000` (0.5 ERG in nanoERG).
4. Confirm.

### 7. Bob sees the received note

In **Bob's** TUI:

1. Go to `Notes → Notes Received`.
2. The note from Alice should be listed with its outstanding amount.

### 8. Redeem the note

Because the TUI currently needs the issuer account locally to co-sign, import
Alice into Bob's wallet first:

```bash
# Get Alice's secret key from her home directory
ALICE_SECRET=$(HOME="$ALICE_HOME" ./target/debug/basis_cli account export alice --json | python3 -c "import sys,json; print(json.load(sys.stdin)['private_key'])")
# Import it into Bob's wallet
HOME="$BOB_HOME" ./target/debug/basis_cli account import alice_redeem "$ALICE_SECRET"
```

Then in **Bob's** TUI:

1. Go to `Notes → Redeem Note`.
2. Select the note from Alice.
3. Enter the amount to redeem (or press Enter for the full outstanding amount).
4. Confirm.

After a successful redemption, Bob's TUI shows a transaction id notification,
and Alice's `My Reserves` screen updates to reflect the reduced collateral.

### 9. (Optional) Emergency redemption

If the normal redemption path fails because the tracker signature is
unavailable, Bob can use `Settings → Tracker Health → Emergency Redemption`.
This bypasses the tracker signature but still requires the issuer's local
co-signature and a confirmed reserve.

### 10. Verify

- In Alice's TUI: `My Reserves` shows reduced collateral and updated liabilities.
- In Bob's TUI: `Notes → Notes Received` shows the redeemed portion, and the
  main-menu stats reflect the change in assets.
- On the Ergo node/explorer: look up the redemption transaction id.

---

## Troubleshooting

| Symptom | Cause / Fix |
|---------|-------------|
| `Server: ○ disconnected` | Tracker URL is wrong or the server is not running. Update it in `Settings → Change Tracker URL`. |
| Note creation rejected | The recipient's acceptance policy does not whitelist the issuer, or `max_debt` is too low. Check/update the policy and ensure it is uploaded to the server. |
| Redemption fails with "no local account for issuer" | The redeemer's wallet needs the issuer account imported (current TUI limitation). |
| Reserve creation fails | The server's Ergo node wallet does not have enough ERG or the tracker NFT. |
| TUI windows overwrite each other | You launched two wallets with the same `HOME`. Always use separate home directories. |

---

## References

- [`BUILD_INSTALL.md`](BUILD_INSTALL.md) — build instructions.
- [`TRACKER_BOX_SETUP.md`](TRACKER_BOX_SETUP.md) — tracker NFT and on-chain setup.
- [`BUILD_AND_CREATE_RESERVE.md`](BUILD_AND_CREATE_RESERVE.md) — reserve creation.
- [`demo/lets_tutorial/README.md`](../demo/lets_tutorial/README.md) — pure-credit LETS demo.
- [`Alice_Bob_Redemption_Test.md`](Alice_Bob_Redemption_Test.md) — older API/curl version.
