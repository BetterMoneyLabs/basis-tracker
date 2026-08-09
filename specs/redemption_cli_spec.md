# Historical v1 CLI redemption specification

> **Status: superseded design reference.** The transaction-generation command
> described by older revisions of this file is removed. It must not be used as
> an implementation, interoperability, or readiness specification.

## Current CLI boundary

- There is no `basis-cli transaction generate-redemption` command.
- The retained `basis-cli note redeem` compatibility command fails before
  account lookup, network access, proof generation, signing, or broadcast.
- The ignored local-sign v1 fixture is removed.
- The TUI exposes no redemption or transaction navigation.

The v2 client accepts either an already constructed exact
`V2RedemptionManifest` or raw JSON through the 2 MiB bounded parser, then
validates it before an opaque callback. Direct serde deserialization is not an
available bypass. It has no concrete prover, signer, wallet adapter, submitter,
or broadcaster, and production manifest construction remains unavailable until
confirmed-chain authority is integrated.

Future CLI redemption must consume only a validated v2 manifest, bind the same
exact reserve and funding boxes through proving and signing, and retain the
source-pinned v2 register/context-extension ABI. It must not restore any v1
request, proof, server-sign, or raw-submit fallback.
