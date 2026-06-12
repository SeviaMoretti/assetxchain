# AssetXChain Agent Development Automation

This document is an execution guide for Codex or other coding agents working on
AssetXChain. It defines how an agent should advance the project safely, one
bounded development step at a time.

## Operating Principles

An agent working on this repository must follow these rules:

- Work from the current codebase, not from assumptions about the original
  Substrate template.
- Keep each task scoped to one module or one user-visible workflow.
- Prefer tests before implementation changes.
- Do not modify unrelated files.
- Do not revert existing user changes unless explicitly instructed.
- Treat generated data, chain databases, and build output as non-source
  artifacts unless the task specifically concerns them.
- When the conversation context has been compacted, reread the available
  conversation context and continue the work from the current state.
- After every implementation step, run the narrowest useful verification first.
- When an automatic task, phase, or scoped work item is completed, update the
  corresponding status in this document before the final report.
- At the end of every automatic task, stage only the files changed by the agent
  for that task with explicit `git add <path>` commands.
- Report exactly what changed, what was verified, and what remains risky.

## Default Agent Loop

For every development task, use this loop:

1. Read the relevant files.
2. Identify the runtime, pallet, node, contract, or documentation boundary.
3. State the intended edit scope.
4. Add or update focused tests where practical.
5. Implement the smallest change that satisfies the task.
6. Run targeted checks.
7. Update the matching phase or work-item status in this document.
8. Inspect the diff.
9. Stage the task files with explicit `git add <path>` commands.
10. Summarize results and next steps.
11. Commit the completed task with a message that starts with `l+`.

If a task requires broader changes than expected, stop and narrow the next unit
of work before editing more modules.

## Git Commit Rule

After each completed task, the agent must commit its own changes to the
repository.

Commit requirements:

- Use a commit message that starts with `l+`.
- Commit only files changed by the agent for the completed task.
- Before committing, stage only those task files with explicit pathspecs; do not
  use `git add .` or include unrelated working-tree changes.
- Do not include unrelated user changes already present in the working tree.
- Do not commit generated data, chain databases, or build output unless the user
  explicitly requested that artifact update.
- If verification was not run, mention that in the final report rather than
  hiding it in the commit message.

Example commit messages:

```text
l+ add project documentation
l+ stabilize dataassets registration tests
l+ implement market certificate transfer extension
```

## Current Priority Order

Agents should advance the project in this order unless the user directs
otherwise.

Status values:

- `Pending`: planned but not started in the current automation sequence.
- `In Progress`: active work is underway.
- `Completed`: completion standard is met and relevant verification passed, or
  the final report states why verification was not applicable.
- `Blocked`: work cannot continue without user input or an external state
  change; include the reason.
- `Unverified`: status was not established when this automation guide was
  updated.

## Current Work Status

- 2026-06-11: `Completed` - Phase 4 IPFS and storage prototype. Kept physical
  data off-chain, added asset metadata fields for off-chain location
  references, implemented the local `storage_ipfs` provider/order/bind/proof
  prototype with provider collateral pledge, and left runtime wiring deferred.
- 2026-06-11: `Completed` - Phase 5 node, RPC, and developer experience docs.
  Documented the current node binary naming decision, custom proof RPC request
  shape, runtime API query examples, `pallet-template` placeholder status, and
  the manual MVP smoke-test flow.
- 2026-06-10: `Completed` - Phase 3 incentives, rewards, and collateral audit.
  Added focused tests for incentive pool accounting, first-create rewards, block
  reward thresholds, collateral pledge/unbond, slash distribution, and runtime
  incentive pool account consistency.
- 2026-06-09: `Completed` - Phase 2 final `pallet-markets` verification unblock.
  `cargo test -p pallet-markets` passed after adding the explicit test-time
  `tokio` dependency.

### Phase 1: Stabilize `pallet-dataassets`

Status: `Unverified`

Goal: make the data asset state machine reliable.

Primary files:

- `pallets/dataassets/src/lib.rs`
- `pallets/dataassets/src/types.rs`
- `pallets/dataassets/src/collateral.rs`
- `pallets/dataassets/src/digest_item.rs`
- `pallets/dataassets/src/tests.rs`

Tasks:

- Add tests for data asset registration.
- Add tests for right-token certificate issuance.
- Add tests for owner transfer and market transfer.
- Add tests for market authorization and revocation.
- Add tests for certificate transfer and revocation.
- Add tests for asset and certificate root updates.
- Separate experiment-only calls from production-facing behavior in
  documentation and tests.

Verification:

```sh
cargo test -p pallet-dataassets
```

Completion standard:

- Core data asset and certificate workflows pass tests.
- Each state transition emits expected events.
- Asset owner and certificate owner changes are covered by tests.
- Authorization is removed after a market transfer.

### Phase 2: Complete Market Transaction MVP

Status: `Completed` (updated 2026-06-09)

Goal: make market-based asset circulation demonstrable end to end.

Primary files:

- `runtime/src/asset_market_extension.rs`
- `pallets/markets/src/lib.rs`
- `contracts/market_standard/lib.rs`
- `contracts/market_orderbook/lib.rs`
- `pallets/dataassets/src/lib.rs`

Tasks:

- Implement certificate transfer behavior in the chain extension, or explicitly
  mark it unsupported with a failing status code.
- Verify the asset transfer function ID and ink! contract binding match.
- Confirm the market account used by `pallet-contracts` is the same account
  stored in `AssetApprovals`.
- Add tests or a smoke procedure for market authorization followed by market
  transfer.
- Clarify the rule for owner transfer while a market authorization exists. The
  preferred rule is: owner transfer is blocked until authorization is revoked.

Verification:

```sh
cargo test -p pallet-dataassets
cargo test -p pallet-markets
```

If contract code changes:

```sh
cargo check -p market-standard
cargo check -p market-orderbook
```

Completion standard:

- A registered or simulated market can transfer an authorized asset.
- The old owner, new owner, and authorization cleanup are verified.
- Failed market transfer does not leave partial runtime state.

### Phase 3: Audit Incentives, Rewards, And Collateral

Status: `Completed` (updated 2026-06-10)

Goal: make the economic accounting internally consistent.

Primary files:

- `pallets/incentive/src/lib.rs`
- `pallets/rewards/src/lib.rs`
- `pallets/collaterals/src/lib.rs`
- `runtime/src/configs/mod.rs`
- `runtime/src/genesis_config_presets.rs`

Tasks:

- Verify incentive pool account configuration matches genesis funding.
- Add tests for first-create reward success and duplicate prevention.
- Add tests for insufficient incentive pool balance.
- Replace placeholder market reward account lookup with a real market operator
  lookup when market registry integration is available.
- Add tests for block reward threshold behavior.
- Add tests for collateral pledge, unbond, and slash distribution.
- Clarify the boundary between data asset registration collateral and
  role-based collateral.

Boundary clarification:

- Data asset registration collateral remains owned by `pallet-dataassets`; it is
  tied to asset availability and registration lifecycle.
- Role-based collateral remains owned by `pallet-collaterals`; it is tied to
  market operators, IPFS providers, and governance participants.
- Market reward operator lookup is deferred until the market registry exposes a
  stable operator lookup interface to `pallet-incentive`.

Verification:

```sh
cargo test -p pallet-incentive
cargo test -p pallet-rewards
cargo test -p pallet-collaterals
```

Completion standard:

- Incentive pool `released`, `used`, and `reserved` accounting is testable.
- Block reward issuance respects threshold and max supply rules.
- Slashing distribution has deterministic tests.

### Phase 4: Implement Storage Prototype Scope

Status: `Completed` (updated 2026-06-11)

Goal: keep storage integration from blocking the market MVP.

Primary files:

- `Cargo.toml`
- `pallets/storage_ipfs/src/lib.rs`
- `pallets/storage_ipfs/src/types.rs`
- `pallets/dataassets/src/lib.rs`
- `pallets/dataassets/src/types.rs`
- `pallets/dataassets/tests/register_asset.rs`
- `sto_ipfs.note`

Preferred short-term direction:

- Keep physical data off-chain.
- Store `metadata_cid`, `data_cid`, data size, and encryption metadata as asset
  metadata.
- Do not implement real IPFS availability checks until the core asset and market
  flow is stable.
- Keep `storage_ipfs` as a workspace prototype for compile-time hygiene, but do
  not add it to the runtime pallet composition yet.
- Reserve explicit verifier extension points for future real IPFS availability
  checks and XCM / storage-chain availability checks.

Completed tasks:

- `storage_ipfs` remains a prototype and is not runtime-wired.
- `pallet-dataassets` registration can store off-chain location metadata through
  `register_asset_with_metadata`.
- `storage_ipfs` is included in the workspace so it can be checked directly.
- `storage_ipfs` supports local provider registration with required
  `IpfsProvider` collateral pledge.
- `storage_ipfs` supports local storage order creation, asset-to-provider
  binding, and storage proof submission.
- `IpfsAvailabilityVerifier` and `XcmAvailabilityVerifier` are no-op extension
  points reserved for future real availability checks.
- Runtime wiring, provider unbond/lifecycle management, real IPFS availability
  checks, XCM message dispatch, and storage-chain callbacks remain deferred.

Verification:

```sh
cargo test -p pallet-dataassets
cargo test -p storage_ipfs
cargo check -p storage_ipfs
```

Completion standard:

- Storage scope is documented.
- Asset registration can reference off-chain data location metadata.
- Storage providers must pledge `CollateralRole::IpfsProvider` collateral during
  registration.
- Asset owners can create local storage orders and bind registered providers.
- Bound providers can submit storage proofs.
- Storage work does not alter core ownership transfer semantics.
- `storage_ipfs` compiles as a standalone workspace package.

### Phase 5: Improve Node, RPC, And Developer Experience

Status: `Completed` (updated 2026-06-11)

Goal: make the project easy to run, inspect, and demo.

Primary files:

- `node/src/data_asset_rpc.rs`
- `runtime/src/runtime_api.rs`
- `runtime/src/apis.rs`
- `README.md`
- `docs/project-overview.md`
- `docs/mvp-smoke-test.md`

Completed tasks:

- Document `dataAssets_getAssetProof` with a complete JSON-RPC example.
- Add runtime API query examples.
- Rename the node binary to `assetxchain-node` and update developer commands.
- Clearly label `pallet-template` as a placeholder that is not part of the data
  asset MVP.
- Add a smoke-test document for the MVP flow.

Verification:

```sh
cargo check -p assetxchain-node
```

Completion standard:

- A new developer can start the dev chain from README.
- The asset registration and query path is documented.
- RPC examples match actual method names.

## Task Templates

Use these templates when starting a new agent task.

### Pallet Bugfix Template

1. Read the pallet `lib.rs`, types, tests, and runtime config.
2. Reproduce the issue with a unit test if possible.
3. Patch only the pallet files needed for the behavior.
4. Run that pallet's tests.
5. Inspect diff for unrelated churn.
6. Report fixed behavior and remaining edge cases.

### Runtime Integration Template

1. Read `runtime/src/lib.rs`, `runtime/src/configs/mod.rs`, and the target
   pallet `Cargo.toml`.
2. Confirm the pallet is in workspace dependencies.
3. Confirm `std`, `runtime-benchmarks`, and `try-runtime` features are wired
   consistently if needed.
4. Add runtime config.
5. Add pallet to runtime composition only after config compiles.
6. Run runtime check.

### Contract Integration Template

1. Read the ink! contract and chain extension binding.
2. Read `runtime/src/asset_market_extension.rs`.
3. Confirm function IDs and SCALE input/output types match.
4. Add a runtime-side test or documented smoke scenario.
5. Check contract crates.

### Documentation Update Template

1. Read current README and relevant docs.
2. Update only the documentation files needed.
3. Keep README concise; put long architecture details in `docs/`.
4. Avoid claiming unfinished functionality is production-ready.
5. Link to source files or commands where useful.

## Verification Matrix

Use this matrix to choose checks. Prefer the narrowest check that covers the
change.

| Change area | First check | Broader check |
| --- | --- | --- |
| `pallet-dataassets` | `cargo test -p pallet-dataassets` | `cargo check` |
| `pallet-markets` | `cargo test -p pallet-markets` | `cargo check` |
| `pallet-incentive` | `cargo test -p pallet-incentive` | `cargo check` |
| `pallet-rewards` | `cargo test -p pallet-rewards` | `cargo check` |
| `pallet-collaterals` | `cargo test -p pallet-collaterals` | `cargo check` |
| Runtime config | `cargo check -p assetxchain-runtime` | `cargo check` |
| Node/RPC | `cargo check -p assetxchain-node` | dev-chain smoke test |
| ink! contracts | `cargo check -p market-standard` | `cargo check -p market-orderbook` |
| Documentation | inspect markdown diff | no build required |

## Diff Hygiene Rules

Before finishing, the agent must inspect the diff and check for:

- Unrelated source changes.
- Generated files changed unintentionally.
- Formatting-only churn outside the task scope.
- Runtime version changes without a runtime behavior reason.
- Cargo feature changes that are not explained.
- Test changes that weaken existing coverage.

If unrelated user changes are present in the working tree, leave them alone and
mention that they were not touched.

## Stop Conditions

Stop and ask the user before proceeding if:

- The task requires changing consensus, genesis economics, or token supply
  semantics beyond the requested scope.
- The implementation would invalidate existing experiment scripts or paper
  results.
- The change requires deleting chain data, build artifacts, or generated
  benchmark outputs.
- A module boundary is unclear and two reasonable designs would lead to
  incompatible APIs.

## Expected Final Report

Every agent run should finish with:

- Files changed.
- Behavior changed.
- Checks run.
- Checks not run and why.
- Remaining risks.
- Suggested next step.

Keep the report concise and grounded in the actual diff.
