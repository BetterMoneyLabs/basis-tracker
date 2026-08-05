# TUI Wallet Acceptance Policy Integration Specification

## Overview

This specification defines how the TUI wallet (`basis-ui`) manages acceptance policies for IOU notes. The TUI wallet provides an interactive interface for users to configure their trust preferences, which are then signed and uploaded to the Basis server for enforcement.

## Core Design Principles

1. **Separate Configs**: TUI uses its own config file (`~/.basis/ui.toml`), independent from CLI config
2. **Default Policy**: 100% collateralization required (reject unless fully collateralized)
3. **Community Config**: Optional `community.toml` for manual trust-based policy replacement
4. **Auto-Upload**: Policy automatically signed and uploaded to server on every change
5. **Simple UX**: 1% increments for collateral, blacklist takes precedence, cached reserve data for testing

## Config Files

### TUI Wallet Config

**Location**: `~/.basis/ui.toml`

```toml
server_url = "http://127.0.0.1:3048"
current_account = "alice"

[acceptance]
default = "reject"
root = "my_policy"

[[acceptance.predicates]]
name = "whitelist"
type = "whitelist"
holders = ["02bob...", "02charlie..."]
max_debt = 5000000000

[[acceptance.predicates]]
name = "blacklist"
type = "blacklist"
holders = ["02badactor..."]

[[acceptance.predicates]]
name = "collateral"
type = "collateralization"
min_ratio = 1.0  # 100%

[[acceptance.predicates]]
name = "not_blacklisted"
type = "not"
predicate = "blacklist"

[[acceptance.predicates]]
name = "whitelist_or_collateral"
type = "any_of"
predicates = ["whitelist", "collateral"]

[[acceptance.predicates]]
name = "my_policy"
type = "all_of"
predicates = ["not_blacklisted", "whitelist_or_collateral"]
```

### Community Config

**Location**: `demo/lets_tutorial/config/community.toml` (example template)

```toml
[acceptance]
default = "reject"
root = "community_trust"

[[acceptance.predicates]]
name = "community_members"
type = "whitelist"
holders = [
    "02alice...", "02bob...", "02charlie...",
    "02dave...", "02eve...", "02frank..."
]

[[acceptance.predicates]]
name = "community_trust"
type = "any_of"
predicates = ["community_members"]
```

**Usage**: Copy the `[[acceptance.predicates]]` blocks into `~/.basis/ui.toml` under the `[acceptance]` section, or use the CLI to upload a raw policy file:

```bash
basis-cli acceptance upload --policy-file community.toml
```

The TUI itself does not have a dedicated "load community.toml" command; policies are edited interactively or uploaded from a file via the CLI.

## Default Policy

When no config exists, TUI auto-generates:

```toml
[acceptance]
default = "reject"
root = "require_full_collateral"

[[acceptance.predicates]]
name = "require_full_collateral"
type = "collateralization"
min_ratio = 1.0  # 100%
```

**Behavior**: Reject all notes unless backed by 100% collateral.

## TUI Policy Screen

```
  ACCEPTANCE POLICY
  ─────────────────

  Current Mode: [100% Collateral Required]
  
  [1] Set Collateral Level (0-1000%)
  [2] Add to Whitelist (trust issuer)
  [3] Remove from Whitelist
  [4] Add to Blacklist (block issuer)
  [5] Remove from Blacklist
  [6] Reset to Default (100% Collateral)
  [7] View Current Policy
  [8] Test Policy Against Issuer

  [B] Back to Menu
```

## Command Flows

### [1] Set Collateral Level

```
Enter collateral percentage (0-1000, default=100): 150

→ Updates policy to require 150% collateral
→ Auto-uploads to server
→ Shows: "✅ Policy updated: 150% collateral required"
```

**Implementation**:
- Parse integer 0-1000
- Update `collateral` predicate `min_ratio = value / 100.0`
- Regenerate composite policy
- Save to `ui.toml`
- Sign and upload to server

### [2] Add to Whitelist

```
Add issuer to whitelist:
  [1] Select from Address Book
  [2] Enter pubkey manually
> 1

Select contact:
  [1] bob      03af13e3...2cea
  [2] charlie  02a3b5c7...3b5c
> 1

Add debt limit? (nanoERG, Press Enter for none): 5000000000

→ Adds bob to whitelist with 5 ERG debt limit
→ Auto-uploads to server
→ Shows: "✅ Added 'bob' to whitelist (limit: 5 ERG)"
```

**Implementation**:
- Lookup pubkey from address book or parse manual input
- Validate 66 hex chars
- Optional: parse debt limit (u64 nanoERG)
- Update `whitelist` predicate holders
- Regenerate composite policy
- Save to `ui.toml`
- Sign and upload to server

### [3] Remove from Whitelist

```
Select issuer to remove:
  [1] bob      03af13e3...2cea  (limit: 5 ERG)
  [2] charlie  02a3b5c7...3b5c  (no limit)
> 1

→ Removes bob from whitelist
→ Auto-uploads to server
→ Shows: "✅ Removed 'bob' from whitelist"
```

### [4] Add to Blacklist

```
Add issuer to blacklist:
  [1] Select from Address Book
  [2] Enter pubkey manually
> 2

Enter pubkey (66 hex chars): 02badactor...

→ Adds to blacklist
→ Auto-uploads to server
→ Shows: "✅ Added to blacklist"
```

**Note**: Blacklist takes precedence. A blacklisted issuer is always rejected, even if whitelisted or collateralized.

### [5] Remove from Blacklist

```
Select issuer to remove from blacklist:
  [1] 02badactor...
> 1

→ Removes from blacklist
→ Auto-uploads to server
→ Shows: "✅ Removed from blacklist"
```

### [6] Reset to Default

```
Reset to 100% collateral required?
  [Y] Yes
  [N] Cancel
> Y

→ Clears whitelist/blacklist, sets collateral to 100%
→ Auto-uploads to server
→ Shows: "✅ Reset to 100% Collateral Required"
```

### [7] View Current Policy

```
Current Policy:
  Default: Reject
  Collateral: 100%
  
  Whitelist (3):
  [1] bob      03af13e3...2cea  (limit: 5 ERG)
  [2] charlie  02a3b5c7...3b5c  (no limit)
  [3] dave     02dave...        (limit: 2 ERG)

  Blacklist (1):
  [1] 02badactor...

  Policy Logic:
  NOT blacklisted AND (whitelisted OR collateralized)

  Press Enter to continue...
```

### [8] Test Policy

```
Test issuer acceptance:
Enter issuer pubkey (or contact name): dave

Result: ✅ ACCEPTED
  Reason: In whitelist (no debt limit)

Enter issuer pubkey (or contact name): eve

Result: ❌ REJECTED
  Reason: Not in whitelist, collateral insufficient

Enter issuer pubkey (or contact name): bob

Result: ❌ REJECTED
  Reason: Blacklisted (blacklist takes precedence)
```

**Implementation**:
- Parse pubkey or lookup from address book
- Build `PredicateContext` with:
  - `issuer_pubkey`: parsed pubkey
  - `recipient_pubkey`: current account pubkey
  - `total_debt`: 0 (for testing, or ask user)
  - `reserve_tracker`: cached data (see below)
- Evaluate policy locally
- Show result with reason

## Reserve Data Caching for Policy Testing

For the `[8] Test Policy` command, the TUI currently performs a local whitelist/blacklist check only:

- Looks up the issuer in the configured whitelist and blacklist predicates.
- Reports whether the issuer would be accepted based on those lists.
- Full server-side evaluation (including collateralization) is done by the tracker's
  `POST /acceptance/check` endpoint; the TUI test is a quick local sanity check.

A future enhancement may add reserve-data caching so the TUI can also estimate
 collateralization locally without a server round-trip on every test.

## Policy Evaluation Logic

The TUI generates a composite policy with this structure:

```
NOT blacklisted AND (whitelisted OR collateralized)
```

In TOML:
```toml
[[acceptance.predicates]]
name = "my_policy"
type = "all_of"
predicates = ["not_blacklisted", "whitelist_or_collateral"]

[[acceptance.predicates]]
name = "not_blacklisted"
type = "not"
predicate = "blacklist"

[[acceptance.predicates]]
name = "whitelist_or_collateral"
type = "any_of"
predicates = ["whitelist", "collateral"]
```

**Precedence**:
1. Blacklist check first — if blacklisted, always reject
2. Whitelist check — if whitelisted and within debt limit, accept
3. Collateral check — if sufficiently collateralized, accept
4. Default policy — reject

## Auto-Upload on Every Change

Whenever policy changes in TUI:

```rust
async fn upload_policy_to_server(&mut self) -> Result<()> {
    // 1. Serialize AcceptanceConfig to canonical JSON
    let policy_json = serde_json::to_string(&self.acceptance_config)?;
    
    // 2. Sign with current account's private key (Schnorr, 65 bytes)
    let signature = self.current_account.sign_message(policy_json.as_bytes())?;
    
    // 3. POST to server
    let request = UploadPolicyRequest {
        recipient_pubkey: self.current_account.pubkey.clone(),
        policy_json,
        signature: hex::encode(signature),
    };
    
    self.client.upload_policy(request).await?;
    
    // 4. Show brief confirmation
    self.set_notification("✅ Policy uploaded to server".to_string(), false);
    
    Ok(())
}
```

**Error Handling**:
- If upload fails: show error, keep local changes, retry on next change
- If no server connection: show "⚠️ Policy saved locally, will upload when connected"

## Server API Changes

### POST /acceptance/policy

Upload a signed acceptance policy for the recipient.

**Request**:
```json
{
  "recipient_pubkey": "02alice...",
  "policy_json": "{...}",
  "signature": "a1b2c3..."
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "uploaded_at": 1234567890,
    "policy_hash": "abc123..."
  }
}
```

**Server Verification**:
1. Verify Schnorr signature on `policy_json` using `recipient_pubkey`
2. Parse `policy_json` into `AcceptanceConfig`
3. Validate predicate structure (no circular references, valid pubkeys)
4. Store in `acceptance_policies` table: `(recipient_pubkey, policy_json, signature, uploaded_at)`

### GET /acceptance/policy/{recipient_pubkey}

Retrieve stored policy for a recipient.

**Response**:
```json
{
  "success": true,
  "data": {
    "recipient_pubkey": "02alice...",
    "policy_json": "{...}",
    "uploaded_at": 1234567890
  }
}
```

### Updated POST /acceptance/check

Check if a note would be accepted by the recipient's policy.

**Request**:
```json
{
  "issuer_pubkey": "02bob...",
  "recipient_pubkey": "02alice...",
  "total_debt": 5000000000
}
```

**Server Logic**:
1. Look up `recipient_pubkey` in `acceptance_policies` table
2. If found: evaluate using stored policy
3. If not found: use server default policy
4. Return result

**Response**:
```json
{
  "success": true,
  "data": {
    "acceptable": true,
    "reason": null
  }
}
```

## Data Model Changes

### basis_app/src/app.rs

```rust
pub struct App {
    // ... existing fields ...
    pub acceptance_config: AcceptanceConfig,
    pub policy_uploaded: bool,
}

// Add new screen
pub enum Screen {
    // ... existing screens ...
    AcceptancePolicy,
}
```

### basis_app/src/ui.rs

Add `draw_acceptance_policy()` function with the 8 interactive commands.

### basis_cli/src/api.rs

```rust
pub async fn upload_policy(&self, request: UploadPolicyRequest) -> Result<UploadPolicyResponse> {
    let url = format!("{}/acceptance/policy", self.base_url);
    let response = ureq::post(&url)
        .send_json(serde_json::to_value(request)?)?;
    
    if response.status() == 200 {
        let api_response: ApiResponse<UploadPolicyResponse> = response.into_json()?;
        if api_response.success {
            Ok(api_response.data.unwrap())
        } else {
            Err(anyhow::anyhow!("Failed to upload policy: {:?}", api_response.error))
        }
    } else {
        Err(anyhow::anyhow!("Failed to upload policy: {}", response.status()))
    }
}
```

### basis_cli/src/commands/acceptance.rs

Add `acceptance upload` and `acceptance check` subcommands for scriptable policy management.

## Files to Modify

| File | Change |
|------|--------|
| `basis_core/src/lib.rs` | Add shared `AcceptanceConfig`, `PredicateConfig`, `DefaultPolicy` |
| `basis_server/src/acceptance/config.rs` | Re-export from `basis_core` |
| `basis_app/src/app.rs` | Add `AcceptanceConfig`, `Screen::AcceptancePolicy` |
| `basis_app/src/ui.rs` | Add `draw_acceptance_policy()` with 8 interactive commands |
| `basis_cli/src/api.rs` | Add `upload_policy()`, `get_policy()`, `check_acceptance()` |
| `basis_cli/src/commands/acceptance.rs` | Add `acceptance upload` / `acceptance check` CLI commands |
| `basis_server/src/api.rs` | Add `POST /acceptance/policy`, `GET /acceptance/policy/{pubkey}` |
| `basis_server/src/models.rs` | Add `UploadPolicyRequest`, `UploadPolicyResponse` |
| `basis_store/src/` | Add `acceptance_policies` table |

## Implementation Order

1. **Move shared types** to `basis_core`
2. **Add server storage** for `acceptance_policies`
3. **Add server API** endpoints (`POST /acceptance/policy`, `GET /acceptance/policy/{pubkey}`)
4. **Update `POST /acceptance/check`** to look up per-recipient policies
5. **Add TUI data model** (`AcceptanceConfig`)
6. **Add TUI API client** (`upload_policy()`)
7. **Add TUI policy screen** with 8 interactive commands
8. **Add CLI acceptance commands** (`acceptance upload`, `acceptance check`)
9. **Add MainMenu** option for Acceptance Policy
10. **Test end-to-end** flow

## Example Data Flow

```
Alice opens TUI:
  → Loads ~/.basis/ui.toml (default: 100% collateral)
  → Shows policy screen

Alice sets collateral to 150%:
  → Updates collateral predicate min_ratio to 1.5
  → Regenerates composite policy
  → Saves to ~/.basis/ui.toml
  → Signs policy with alice's key
  → POST /acceptance/policy to server
  → Server verifies signature, stores in DB
  → Shows: "✅ Policy uploaded: 150% collateral required"

Bob (under-collateralized) tries to pay Alice:
  → Server looks up Alice's policy
  → Bob's reserve has 80% collateral
  → 80% < 150% required
  → Rejects ❌

Charlie (fully collateralized) tries to pay Alice:
  → Server looks up Alice's policy
  → Charlie's reserve has 200% collateral
  → 200% >= 150% required
  → Accepts ✓

Dave (whitelisted by Alice) tries to pay Alice:
  → Server looks up Alice's policy
  → Dave is in whitelist
  → Accepts ✓ (no collateral check needed)

Eve (blacklisted by Alice) tries to pay Alice:
  → Server looks up Alice's policy
  → Eve is in blacklist
  → Rejects ❌ (blacklist takes precedence)
```

## References

- `specs/acceptance_predicates.md` — Server-side acceptance predicate specification
- `specs/tui_wallet_lets.md` — LETS/community-currency demo using acceptance policies
- `crates/basis_server/src/acceptance/` — Server implementation
- `crates/basis_app/src/ui.rs` — TUI wallet UI
- `crates/basis_cli/src/commands/acceptance.rs` — CLI acceptance commands
- `crates/basis_cli/src/api.rs` — API client