# Cross-Repository Rules

Every Constellation project ships as three repositories: a protocol repository, an SDK repository, and an application repository. This document is the contract between them. It is copied into each repository as `docs/multi-repo.md` and linked from every `CONTRIBUTING.md`.

---

## 1. Dependency direction

```
protocol repo  ->  SDK repo  ->  application repo
```

Dependencies point one way only.

- The protocol repository depends on nothing else in the project. It must build and test with no knowledge of the SDK or the app.
- The SDK repository depends on the protocol repository's published artifacts: the contract interface, the generated bindings, and the deployed contract IDs. It never imports application code.
- The application repository depends on the published SDK package. It never talks to a contract directly, and never re-implements SDK logic. If the app needs something the SDK cannot do, the fix goes in the SDK.

A pull request that reverses this direction is rejected on sight. It is the one rule that keeps three repositories from becoming a monorepo with extra steps.

## 2. How contract IDs travel

The protocol repository owns deployment truth. It publishes `deployments/testnet.json`:

```json
{
  "network": "testnet",
  "networkPassphrase": "Test SDF Network ; September 2015",
  "deployedAt": "2026-09-20T10:14:00Z",
  "commit": "a1b2c3d",
  "contracts": {
    "attestation": "CB7X...",
    "schema_registry": "CDQ2..."
  }
}
```

The SDK repository vendors that file at build time and exports it as `deployments`, so an application never hardcodes a contract ID:

```ts
import { deployments } from "@constellation/astrolabe-sdk";
```

A redeployment is therefore a three-step release: deploy from the protocol repository, bump the SDK, bump the app. Never edit a contract ID by hand in an app.

## 3. Versioning

- **Protocol repositories** use git tags: `v0.3.0`. A tag builds the WASM, attaches it to a GitHub release with its sha256, and publishes the generated bindings package.
- **SDK repositories** use semantic versioning on npm. The major version tracks protocol compatibility: SDK `1.x` works with protocol `v1.x` and says so in its README compatibility table.
- **Application repositories** are not versioned for consumers. They deploy from `main` and tag releases for changelog purposes only.
- Before 1.0, breaking changes are allowed in minors, and the SDK README compatibility table is the source of truth.

Every SDK README carries this table and it must be updated in the same pull request that changes compatibility:

| SDK version | Protocol version | Testnet | Mainnet |
|---|---|---|---|
| 0.2.x | v0.3.0 | yes | no |

## 4. Landing a change that spans repositories

Open the pull requests in dependency order and link them to each other.

1. Protocol pull request. Merge, tag a prerelease, deploy to Testnet.
2. SDK pull request. Point at the new prerelease, publish an SDK prerelease (`0.3.0-rc.1`).
3. Application pull request. Point at the SDK prerelease, verify the flow on Testnet.
4. Promote all three from prerelease to release in the same order.

Each pull request states in its description which repositories it depends on and links them. A reviewer should never have to guess.

## 5. Local development across repositories

You do not need all three checked out. Most contributions touch one.

When you do need two, use a workspace link rather than publishing:

```bash
# in the SDK repo
pnpm build && pnpm link --global

# in the app repo
pnpm link --global @constellation/<project>-sdk
pnpm dev
```

For contract work with an app in front of it:

```bash
# in the protocol repo
./scripts/deploy_testnet.sh          # writes deployments/local-testnet.json
# in the app repo
CONSTELLATION_DEPLOYMENTS=../<project>-contracts/deployments/local-testnet.json pnpm dev
```

Every application repository reads that environment variable. If it does not, that is a bug worth filing.

## 6. Where an issue belongs

- The bug is in on-chain behaviour, an error code, storage, or gas: **protocol repository**.
- The bug is in types, encoding, decoding, retries, or the client interface: **SDK repository**.
- The bug is in a screen, a service, a flow, or a deployment: **application repository**.
- You cannot tell: open it in the application repository, where most reports arrive, and a maintainer will transfer it. GitHub keeps the thread when transferring inside an organisation.

Each repository carries the same difficulty labels — `good first issue`, `intermediate`, `advanced` — plus its own area labels.

## 7. Shared CI expectations

Protocol repositories:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --target wasm32-unknown-unknown --release
```

SDK and application repositories:

```
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

Integration tests that hit Testnet are skipped unless a funded key is present in `STELLAR_SECRET`. They run on a schedule in CI, not on every pull request, because Testnet resets and rate limits make them flaky as a gate.

## 8. Security

Every repository carries `SECURITY.md` with the same private reporting address. Report a vulnerability there, never in a public issue. Protocol repositories additionally carry `docs/threat-model.md`, and any pull request that changes an authorisation check must say in its description which threat-model item it affects.

All six projects are unaudited and Testnet-only until stated otherwise in the protocol repository's README.

## 9. Governance

- Two merged pull requests in a repository earns triage rights there on request.
- Commit rights are per repository. Application repositories grant them readily. Protocol repositories grant them slowly and only after contract review work.
- Protocol changes need a written proposal in the protocol repository's Discussions and one maintainer sign-off before implementation begins.
