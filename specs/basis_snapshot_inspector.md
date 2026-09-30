# Basis Snapshot Inspector

**Status:** Proposal. The commands and endpoints below define target behavior
and are not part of the current release.

## Purpose

The Basis Snapshot Inspector is a read-only diagnostic tool for one issuer and
its selected creditor relations. It verifies the internal consistency of the
state and cryptographic evidence supplied by a Basis tracker, then reports the
claims that the evidence does and does not support.

The proposed default entry point is one command:

```console
basis-cli inspector <issuer-pubkey>
```

The command will fetch a bounded snapshot from the configured tracker, verify
it locally and print a short report. Users will not need to create or edit
JSON.

A canonical JSON snapshot will remain available as a portable evidence bundle.
It can be attached to a bug report, sent to another reviewer or replayed
without access to the original tracker:

```console
basis-cli inspector <issuer-pubkey> --save-snapshot snapshot.json
basis-snapshot-inspector inspect snapshot.json
```

## Questions answered

For every relation included in the snapshot, the Inspector can answer:

- which local, pending and confirmed cumulative debt values were supplied;
- whether an issuer or tracker Schnorr signature verifies for the exact derived
  message;
- whether a supplied AVL proof authenticates the derived key and value against
  the supplied root;
- whether supplied contract bytes equal the reference bytes named by a
  recognized model manifest;
- whether issuer, creditor, tracker, reserve and asset identities agree;
- which calculations are unavailable because a required fact or proof is
  missing.

The Inspector keeps cumulative debt, cumulative redeemed value and supplied
reserve observations separate. It calculates a remaining claim only when the
required cumulative values are coherent and checked subtraction succeeds.

## Evidence boundary

Each verified check establishes only the named consistency with the supplied
evidence and recognized model manifest. The report can also contain failed or
unavailable checks. It does not establish:

- active-chain inclusion or canonical-chain selection;
- observation freshness or finality;
- issuer solvency or the completeness of its liabilities;
- redeemable capacity or transaction feasibility;
- the availability or honesty of a tracker;
- deployment safety, legal compliance or creditworthiness.

These limits are part of the machine-readable report. A missing proof is
reported as missing evidence, never converted into a failed or successful
verification.

## User output

The default report is intended for triage and should fit on one terminal
screen. Failures appear before supporting details.

```text
Scope: 1 selected relation; issuer-wide totals unavailable

Creditor 03ab…91ef
Debt: note 100 | confirmed 100 | redeemed 20 | remaining 80 nanoERG
Evidence: issuer signature verified | tracker AVL verified
Reserve entry: supplied as known absent; non-membership not verified
Contract bytes: match recognized model manifest

Failures: none
Unavailable: chain currentness, finality, solvency, redeemability
```

A verbose mode will print the full rule-by-rule report. A JSON mode will return
the typed report for CI, MCP clients and other programs. Hexadecimal proof
material will be omitted from the default human view.

## Snapshot model

Every snapshot binds the following data:

```text
schema_version
ruleset_version
snapshot_id
model_pin
observation
subject
liability_scope
relations[]
reserve_observation
tracker_state_observation
tracker_availability_observation
evidence?
```

`model_pin` identifies a reviewed manifest containing the contract variant,
source revision and digest, compiler and ErgoTree versions, and expected
serialized ErgoTree bytes with their digest. Contract-byte comparison is
unavailable when that exact manifest is absent or unrecognized.

The subject is one issuer, tracker NFT and asset tuple. Each relation is bound
to one creditor public key. Evidence references use that public key rather than
array position.

`liability_scope` is one of:

- `complete_for_subject`, a producer-declared complete enumeration bound to one
  coherent tracker state and evidence reference;
- `selected_relations`, meaning only the returned relations were inspected;
- `unknown`.

The proposed initial tracker export always uses `selected_relations`.
`complete_for_subject` is reserved for a producer that supplies a declared
complete enumeration bound to one coherent tracker state and evidence
reference. The Inspector never infers completeness. Issuer-wide totals and
coverage are not calculated for partial or unknown scope.

Reserve observations distinguish `observed`, a scoped `known_none` assertion
and `not_observed`. Tracker-state observations are `observed` or
`not_observed`; tracker availability is `available`, `unavailable` or
`unknown`. A scoped reserve-absence assertion reports only what its declared
discovery method found. It does not establish current chain state.

## Verification rules

The rule set covers:

1. canonical public keys, identifiers and subject bindings;
2. duplicate relations and conflicting evidence references;
3. non-negative Ergo `Long` bounds and checked arithmetic;
4. differences between note, local, pending and confirmed debt tiers;
5. remaining claim from coherent cumulative debt and redemption values;
6. note timestamp ordering against the bound reserve-ledger timestamp;
7. Schnorr verification with messages reconstructed from validated fields;
8. AVL membership with keys, values and roots reconstructed from the snapshot;
9. exact equality with the contract bytes named by the recognized model
   manifest;
10. suppression of totals derived from partial or mixed successor states.

Every finding has a stable rule identifier, affected path, exact observed
values, evidence basis and authority ceiling. Findings are ordered
deterministically. The tool produces no global safety score.

## Components

The proposal has three separate components:

- `basis_snapshot_inspector`: a deterministic library and offline CLI. It has
  no network, wallet, signing or broadcast capability.
- Basis tracker export: a bounded read-only endpoint that constructs the
  canonical snapshot from tracker and declared node observations.
- `basis-cli inspector`: the user-facing command that fetches the snapshot and
  runs the verification locally. The tracker does not grade its own output.

The library owns parsing, canonicalization, evaluation and rendering. Producers
may construct the same schema from fixtures or other data sources, but they
cannot raise the report's authority above the evidence they include.

## Bounds and failure behavior

These are proposed initial safety bounds, not guarantees of the current
software:

- Input and canonical output are limited to 1 MiB.
- The tracker export returns at most 64 selected relations.
- Proof generation is attempted for at most 16 relations.
- One AVL proof is limited to 64 KiB and cumulative raw proof bytes to 256 KiB.
- Duplicate JSON keys, unknown fields and unsupported schema versions fail
  before evaluation.
- Remote calls have body limits and deadlines.
- Exporting to a file uses a same-directory temporary file and atomic replace;
  a failed write preserves the existing destination.

An omitted optional proof does not omit its relation. Storage/index mismatch,
mixed tracker anchors and oversized evidence fail closed rather than producing
a broader report.

## Acceptance criteria

A conforming implementation satisfies this specification when:

- one command can inspect an issuer without exposing the snapshot format;
- saved snapshots replay offline with byte-identical output under the same
  Inspector and ruleset versions;
- the same valid input always produces the same ordered findings;
- signatures and AVL proofs are checked against values derived by the
  Inspector, not caller-selected verification inputs;
- every rule has a positive case and a single-fault negative case;
- partial relation scope never produces issuer-wide totals;
- `known_none` and `not_observed` remain distinct in text and JSON;
- reports state the unsupported chain and financial conclusions;
- no command reads, creates or transmits wallet or spending private keys;
- tracker authentication credentials never enter snapshots or reports.

Required conformance fixtures include a coherent partial-redemption case, a
scoped reserve-absence assertion and an unavailable-tracker case with divergent
local, pending and confirmed debt.

## Excluded functions

The Inspector does not build or sign transactions, broadcast, remediate state,
set acceptance policy, assign credit scores or forecast future solvency. A
separate integration may later add authenticated chain observations or MCP
transport without changing the offline evaluator's evidence rules.
