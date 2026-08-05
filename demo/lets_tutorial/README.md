# Basis LETS Tutorial — Mutual Credit with the TUI Wallet

A hands-on tutorial that models a **Local Exchange Trading System (LETS)** with the `basis-ui` TUI wallet. It is a pure mutual-credit demo: members trust each other through whitelists, no reserves are created, and no collateral or on-chain redemption is required.

## What is LETS on Basis?

A LETS is a local community where members create common credit money by accepting each other's IOU notes. On Basis:

- Each member runs their own `basis-ui` wallet.
- Membership is expressed through an **acceptance policy** that whitelists all other members.
- Notes are cumulative IOUs tracked by the Basis tracker server.
- Assets, liabilities, and net position are shown on the TUI stats screen.
- No Ergo node, no reserves, and no blockchain fees are needed.

## Prerequisites

- Rust toolchain (same as the rest of the project)
- `bash`, `curl`, `python3`
- Optional: `tmux` for automatic multi-member launch

## Quick Start

Run the setup script from the project root:

```bash
./demo/lets_tutorial/run_lets_tutorial.sh --tmux
```

This builds `basis_server`, `basis_cli`, and `basis-ui`, starts a local tracker, creates three member wallets (alice, bob, carol), and launches each wallet in a separate tmux window.

To set up without tmux (the script prints manual launch commands):

```bash
./demo/lets_tutorial/run_lets_tutorial.sh
```

## Suggested Trading Scenario

After launching the wallets, perform these three payments:

1. **Alice pays Bob 2 ERG** for a loaf of bread.
   - In Alice's TUI: **Create Note** → select `bob` → amount `2000000000` → confirm.
2. **Bob pays Carol 1 ERG** for a ride.
   - In Bob's TUI: **Create Note** → select `carol` → amount `1000000000` → confirm.
3. **Carol pays Alice 1.5 ERG** for tutoring.
   - In Carol's TUI: **Create Note** → select `alice` → amount `1500000000` → confirm.

### Expected Balances

| Member | Assets (received) | Liabilities (issued) | Net position |
|--------|-------------------|----------------------|--------------|
| Alice  | 1.5 ERG           | 2.0 ERG              | -0.5 ERG     |
| Bob    | 2.0 ERG           | 1.0 ERG              | +1.0 ERG     |
| Carol  | 1.0 ERG           | 1.5 ERG              | -0.5 ERG     |

Open the **Wallet Stats** screen in each TUI to watch these values update in real time.

## How It Works

### Member Isolation

Each member has a separate `$HOME` directory under `demo/lets_tutorial/data/<member>/`. This keeps the TUI config (`~/.basis/ui.toml`), CLI config (`~/.basis/cli.toml`), and account files isolated so multiple wallets can run on the same machine.

### Acceptance Policy

The script generates a per-member policy and uploads it to the tracker:

```toml
default = "reject"
root = "lets_trust"

[[predicates]]
name = "lets_members"
type = "whitelist"
holders = ["02bob...", "02carol..."]
max_debt = 5000000000  # 5 ERG credit limit

[[predicates]]
name = "lets_trust"
type = "any_of"
predicates = ["lets_members"]
```

This means: *accept notes only from whitelisted members, and reject everyone else by default*. The optional `max_debt` caps how negative any single member can go.

### Address Book

The script pre-seeds each TUI address book with the other members, so recipients can be selected by name instead of typing a 66-character public key.

## Script Options

```bash
./demo/lets_tutorial/run_lets_tutorial.sh [OPTIONS]

  --members alice,bob,dave   Customize member names (default: alice,bob,carol)
  --credit-limit N           Per-member credit limit in nanoERG (default: 5 ERG)
  --tmux                     Launch wallets in tmux windows
  --clean                    Remove previous demo state
  --release                  Build release binaries
  --help                     Show usage
```

## Manual Steps (without the script)

If you prefer to set everything up manually:

1. Build the binaries:
   ```bash
   cargo build -p basis_server -p basis_cli -p basis_app
   ```
2. Start the tracker server with a custom `config/basis.toml` (set a dummy `tracker_nft_id` and `basis_reserve_contract_p2s`).
3. For each member, create an isolated home directory and run:
   ```bash
   HOME=/path/to/member/home ./target/debug/basis_cli account create <name>
   ```
4. Write a `ui.toml` with a whitelist of all other members and a server URL.
5. Upload the policy:
   ```bash
   HOME=/path/to/member/home ./target/debug/basis_cli acceptance upload \
     --policy-file /path/to/policy.toml
   ```
6. Launch the TUI:
   ```bash
   HOME=/path/to/member/home ./target/debug/basis-ui
   ```

## Troubleshooting

### "Server not connected" in TUI stats

The tracker server is not running or not reachable. Re-run the setup script; it starts the server automatically.

### Notes are rejected

Each member must have a policy uploaded to the tracker. The script does this automatically; if you changed a policy in the TUI, make sure it was uploaded (the TUI saves and uploads on every change in the Acceptance Policy screen).

### Address book is empty

The address book is loaded from `ui.toml` and from local accounts. If you deleted `ui.toml`, add contacts manually in the TUI's Address Book screen.

### TUI windows collide

Each member must use a different `$HOME`. Do not run multiple `basis-ui` instances with the same home directory.

## Extending the Demo

- **More members**: pass `--members alice,bob,carol,dave,eve`.
- **Tighter credit limits**: `--credit-limit 1000000000` for a 1 ERG cap.
- **Collateral mode**: replace the whitelist policy with a collateralization predicate to experiment with backed notes.
- **Redemption flow**: switch to `specs/interactive_demo.md` for the full on-chain reserve/redemption tutorial once you want backed notes and redemption.

## References

- [`run_lets_tutorial.sh`](run_lets_tutorial.sh) — setup script
- [`specs/tui_wallet_lets.md`](../../specs/tui_wallet_lets.md) — design specification
- [`specs/acceptance_predicates.md`](../../specs/acceptance_predicates.md) — acceptance policy reference
- [`demo/agent_coop/README.md`](../agent_coop/README.md) — scripted agent co-op variant
