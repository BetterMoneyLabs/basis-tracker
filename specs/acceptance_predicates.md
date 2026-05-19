# Acceptance Predicate Specification

## Overview

Acceptance predicates determine whether a Basis note (IOU) is acceptable as payment. Each participant defines their own acceptance policy, enabling decentralized, individualized trust decisions. This document specifies the predicate system for the Basis tracker, replicating and extending ChainCash's acceptance behavior.

## Core Concept

When agent A offers a note to agent B as payment, B evaluates the note using their personal acceptance predicate P(note). If P(note) returns true, B accepts the payment; otherwise, it is rejected.

Predicates operate on note data including:
- Note owner (issuer, public key)
- Note receiver (creditor, public key)
- Reserve collateralization status of the owner
- Predefined whitelists and blacklists

## Predicate Interface

```rust
/// Context passed to predicate evaluation
pub struct PredicateContext {
    /// Public key of the note issuer (owner)
    pub issuer_pubkey: PubKey,
    /// Public key of the note recipient (creditor)
    pub recipient_pubkey: PubKey,
    /// Total cumulative debt amount in the note
    pub total_debt: u64,
    /// Optional cloned reserve tracker for collateralization checks
    pub reserve_tracker: Option<ReserveTracker>,
}

/// Trait for note acceptance predicates
pub trait NotePredicate: Send + Sync + std::fmt::Debug {
    /// Evaluate whether a note is acceptable given the context
    fn acceptable(&self, ctx: &PredicateContext) -> bool;
    
    /// Get the predicate name
    fn name(&self) -> &str;
}
```

## Predicate Types

### 1. Whitelist Predicate

Accepts notes only if the owner is in a whitelist. Optionally enforces a maximum cumulative debt limit per whitelisted owner.

**Behavior**:
- If owner's public key is in the whitelist: check optional debt limit
  - If `max_debt` is set and note's `total_debt` exceeds it: REJECT
  - Otherwise: ACCEPT
- Otherwise: REJECT

**Use Case**: Trusted counterparties with optional credit limits. In LETS systems, members may be whitelisted with balance caps to prevent unlimited negative balances.

### 2. Blacklist Predicate

Rejects notes if the owner is in a blacklist.

**Behavior**:
- If owner's public key is in the blacklist: REJECT
- Otherwise: ACCEPT

**Use Case**: Block known bad actors, sanctions lists, disputed accounts.

### 3. Collateralization Predicate

Accepts notes based on reserve collateralization ratios.

**Behavior**:
For a given note, compute:
```
owner_reserve     = reserve associated with note owner (issuer)
assets            = owner_reserve.value (in nanoERG)
liabilities       = owner_reserve.total_issued_debt (in nanoERG)

accept if: assets >= liabilities * min_ratio
```

**Variants**:
- **Full collateralization** (ratio = 1.0): Assets ≥ Liabilities
- **Over-collateralization** (ratio > 1.0): Assets ≥ Liabilities * ratio
- **Partial collateralization** (ratio < 1.0): Accepts under-collateralized notes

**Use Case**: Risk-based acceptance, requiring minimum backing for IOUs.

### 4. Composite Predicates

Combine multiple predicates using logical operators.

**Operators**:
- **AND** (`AllOf`): All sub-predicates must pass
- **OR** (`AnyOf`): At least one sub-predicate must pass
- **NOT** (`Not`): Inverts the predicate result

## TOML Configuration Format

Acceptance predicates are configured via TOML files loaded by the Basis server.

### Basic Structure

```toml
[acceptance]
# Default policy if no specific predicate matches
default = "reject"

# Named predicate definitions
[[acceptance.predicates]]
name = "trusted_holders"
type = "whitelist"
holders = [
    "02a1b2c3d4e5f6...",  # hex-encoded 33-byte compressed public keys
    "03f6e5d4c3b2a1...",
]

[[acceptance.predicates]]
name = "sanctions_list"
type = "blacklist"
holders = [
    "02badactor0000...",
]

[[acceptance.predicates]]
name = "fully_collateralized"
type = "collateralization"
min_ratio = 1.0  # 100% collateralization required

[[acceptance.predicates]]
name = "generous_policy"
type = "collateralization"
min_ratio = 0.5  # 50% collateralization acceptable

# Composite predicate combining others
[[acceptance.predicates]]
name = "my_policy"
type = "any_of"  # Logical OR
predicates = ["trusted_holders", "fully_collateralized"]

[[acceptance.predicates]]
name = "strict_policy"
type = "all_of"  # Logical AND
predicates = ["fully_collateralized", "sanctions_list"]
# Note: blacklist in AND means "not in blacklist" - see negation below

[[acceptance.predicates]]
name = "safe_holders"
type = "not"
predicate = "sanctions_list"
```

### Configuration Schema

```toml
# Top-level acceptance section
[acceptance]
# Default behavior when no predicate is configured
default = "accept" | "reject"
# Optional: name of the root predicate to evaluate
# If not specified, the last predicate in the list is used as root
root = "lets_policy"

# Predicate array - order matters for evaluation
[[acceptance.predicates]]
name = "<string>"           # Unique identifier for the predicate
type = "<predicate_type>"   # One of: whitelist, blacklist, collateralization, all_of, any_of, not

# For whitelist type:
holders = ["<33-byte hex pubkey>", ...]  # List of compressed secp256k1 public keys
max_debt = <u64>           # Optional: maximum cumulative debt (nanoERG) per whitelisted owner

# For blacklist type:
holders = ["<33-byte hex pubkey>", ...]  # List of compressed secp256k1 public keys

# For collateralization type:
min_ratio = <f64>           # Minimum collateralization ratio (e.g., 1.0 = 100%)

# For all_of/any_of types:
predicates = ["<name>", ...]  # References to other named predicates

# For not type:
predicate = "<name>"        # Single predicate to negate
```

## ChainCash CoW1 Equivalence

The ChainCash "Collateral or Whitelist #1" (CoW1) predicate is represented as:

```toml
[acceptance]
root = "cow1"

[[acceptance.predicates]]
name = "cow1"
type = "any_of"
predicates = ["trusted_holders", "full_collateral"]

[[acceptance.predicates]]
name = "trusted_holders"
type = "whitelist"
holders = ["02a1b2c3...", "03d4e5f6..."]

[[acceptance.predicates]]
name = "full_collateral"
type = "collateralization"
min_ratio = 1.0
```

This accepts notes if either:
- The owner is whitelisted, OR
- The note is backed by ≥100% collateral

## Evaluation Algorithm

```rust
/// Build predicate tree from config and evaluate a note
fn evaluate(config: &AcceptanceConfig, ctx: &PredicateContext) -> bool {
    // Build the predicate tree from configuration
    let predicate = build_predicate_tree(config);
    
    // Evaluate against the root predicate (or fall back to default)
    match predicate {
        Some(pred) => pred.acceptable(ctx),
        None => config.default.acceptable(),
    }
}
```

### Collateralization Calculation

```rust
fn check_collateralization(ctx: &PredicateContext, min_ratio: f64) -> bool {
    let tracker = match &ctx.reserve_tracker {
        Some(t) => t,
        None => return false,
    };
    
    let reserve = match tracker.get_reserve_by_owner(
        &hex::encode(&ctx.issuer_pubkey)) {
        Ok(r) => r,
        Err(_) => return false,
    };
    
    let assets = reserve.base_info.collateral_amount;
    let liabilities = reserve.total_debt;
    
    if liabilities == 0 {
        return true;  // No debt means fully collateralized
    }
    
    let ratio = assets as f64 / liabilities as f64;
    ratio >= min_ratio
}
```

## Examples

### Example 1: Strict Business Policy

```toml
[acceptance]
default = "reject"
root = "business_policy"

[[acceptance.predicates]]
name = "not_sanctioned"
type = "not"
predicate = "sanctions"

[[acceptance.predicates]]
name = "sanctions"
type = "blacklist"
holders = ["02bad1...", "03bad2..."]

[[acceptance.predicates]]
name = "well_collateralized"
type = "collateralization"
min_ratio = 1.5  # 150% collateral required

[[acceptance.predicates]]
name = "business_policy"
type = "all_of"
predicates = ["not_sanctioned", "well_collateralized"]
```

### Example 2: Permissive with Trust

```toml
[acceptance]
default = "accept"

[[acceptance.predicates]]
name = "blocked"
type = "blacklist"
holders = ["02blocked..."]

[[acceptance.predicates]]
name = "friends"
type = "whitelist"
holders = ["02alice...", "02bob..."]

[[acceptance.predicates]]
name = "my_policy"
type = "all_of"
predicates = ["not_blocked", "friends_or_collateral"]

[[acceptance.predicates]]
name = "not_blocked"
type = "not"
predicate = "blocked"

[[acceptance.predicates]]
name = "friends_or_collateral"
type = "any_of"
predicates = ["friends", "min_collateral"]

[[acceptance.predicates]]
name = "min_collateral"
type = "collateralization"
min_ratio = 0.8  # 80% collateral acceptable
```

### Example 3: Trust-Only (No Collateral Check)

```toml
[acceptance]
default = "reject"

[[acceptance.predicates]]
name = "trusted"
type = "whitelist"
holders = ["02partner1...", "02partner2...", "02partner3..."]
```

### Example 4: Collateral-Only (No Trust)

```toml
[acceptance]
default = "reject"

[[acceptance.predicates]]
name = "fully_backed"
type = "collateralization"
min_ratio = 1.0
```

### Example 5: Local Exchange Trading System (LETS)

A LETS is a local mutual credit association where members create common credit money individually. On Basis, LETS members whitelist each other so they accept notes regardless of collateralization.

```toml
[acceptance]
default = "reject"
root = "lets_policy"

[[acceptance.predicates]]
name = "lets_members"
type = "whitelist"
holders = [
    "02alice...", "02bob...", "02charlie...",
    "02dave...", "02eve...", "02frank..."
]

[[acceptance.predicates]]
name = "lets_policy"
type = "any_of"
predicates = ["lets_members"]
```

**LETS Properties**:
- Members decided off-chain via community governance
- No minimum collateralization required (reserves only need storage rent ~0.001 ERG)
- Notes circulate purely on mutual trust within the community
- Alice's balance at any time: total value of notes she holds minus total value of all notes she ever issued (may be negative)

**LETS with Debt Limits**:

To prevent unlimited negative balances, each member can have a maximum cumulative debt:

```toml
[acceptance]
default = "reject"
root = "lets_policy"

[[acceptance.predicates]]
name = "lets_members"
type = "whitelist"
holders = [
    "02alice...", "02bob...", "02charlie...",
    "02dave...", "02eve...", "02frank..."
]
max_debt = 5000000000  # 5 ERG maximum cumulative debt per member

[[acceptance.predicates]]
name = "lets_policy"
type = "any_of"
predicates = ["lets_members"]
```

If Alice's cumulative debt (totalDebt in the note) exceeds 5 ERG, her notes will be rejected even though she is whitelisted.

**Municipality Endorsement Extension**:

A note endorsed by a trusted authority (e.g., local municipality) can be accepted by non-LETS parties:

```toml
[acceptance]
default = "reject"
root = "endorsed_note"

[[acceptance.predicates]]
name = "lets_members"
type = "whitelist"
holders = ["02alice...", "02bob...", "02charlie..."]

[[acceptance.predicates]]
name = "municipality"
type = "whitelist"
holders = ["02municipality_key..."]

[[acceptance.predicates]]
name = "endorsed_note"
type = "any_of"
predicates = ["lets_members", "municipality"]
```

A non-LETS merchant can accept notes that either:
- Were issued by a LETS member they trust, OR
- Carry a municipality signature endorsing the note

This creates a bridge between the trust-based LETS economy and the broader collateralized economy.

## API Endpoint

### POST /acceptance/check

Check if a note would be accepted by the server's acceptance policy.

**Request**:
```json
{
  "issuer_pubkey": "02a1b2c3...",  // Hex-encoded 33-byte compressed public key
  "total_debt": 5000000000         // Total cumulative debt in nanoERG
}
```

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

If the note is rejected, `acceptable` is `false` and `reason` contains the explanation:
```json
{
  "success": true,
  "data": {
    "acceptable": false,
    "reason": "Note rejected by 'lets_policy' policy"
  }
}
```

**Error cases**:
- `400 Bad Request`: Invalid hex encoding or wrong pubkey length
- `500 Internal Server Error`: Server error

## Implementation

### Source Files

- **`crates/basis_server/src/acceptance/mod.rs`** - Core trait and predicate implementations
- **`crates/basis_server/src/acceptance/config.rs`** - TOML configuration structures
- **`crates/basis_server/src/acceptance/builder.rs`** - Predicate tree builder with circular reference detection
- **`crates/basis_server/src/api.rs`** - `check_acceptance` API endpoint
- **`crates/basis_server/src/models.rs`** - Request/response models

### Integration

Acceptance predicates are:
1. Loaded from `config/basis.toml` at server startup
2. Built into a predicate tree via `build_predicate_tree()`
3. Stored in `AppState.acceptance_predicate: Option<Arc<dyn NotePredicate>>`
4. Evaluated by `POST /acceptance/check` endpoint

### Storage

- Predicates are loaded at server startup from TOML configuration
- Public keys in whitelists/blacklists are stored as 33-byte compressed secp256k1 keys
- Collateralization state is derived from tracked reserves in real-time via `ReserveTracker`

### Performance

- Whitelist/blacklist checks: O(1) with HashSet
- Collateralization checks: O(1) - single reserve lookup for note owner
- Composite predicates: Depth-first evaluation with short-circuiting
- Predicate tree built once at startup, not per-request

### Security

- Predicate configuration is server-local and not shared on-chain
- Acceptance decisions are client-side; no central authority enforces rules
- A note rejected by B may still be accepted by C
- Collateralization ratios depend on accurate reserve tracking
- Fail-safe: missing reserve → reject note

## Future Extensions

Potential predicate types for future implementation:

- **Issuer Predicate**: Accept only notes from specific issuers
- **Age Predicate**: Accept only notes newer/older than a threshold
- **Amount Predicate**: Accept only notes within a value range
- **Custom Script Predicate**: User-defined evaluation logic

## References

- ChainCash NotePredicate.scala: `chaincash/offchain/NotePredicate.scala`
- ChainCash Server Documentation: `chaincash/docs/server.md`
- Basis Protocol Specification: `specs/spec.md`
