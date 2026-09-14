#!/usr/bin/env bash
# Deploy the programme contract to Testnet and record the address as deployment
# truth. The SDK vendors the file this writes; never edit a contract id by hand
# downstream.
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
SOURCE="${SOURCE:-cygnus-deployer}"
OUT="${OUT:-deployments/testnet.json}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "Building contract for $NETWORK..."
stellar contract build >/dev/null
WASM="target/wasm32v1-none/release/cygnus_programme.wasm"

echo "Deploying programme contract..."
CONTRACT_ID="$(stellar contract deploy --wasm "$WASM" --source "$SOURCE" --network "$NETWORK")"
echo "Programme contract: $CONTRACT_ID"

COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo uncommitted)"
DEPLOYED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
PASSPHRASE="Test SDF Network ; September 2015"

mkdir -p "$(dirname "$OUT")"
cat > "$OUT" <<EOF
{
  "network": "$NETWORK",
  "networkPassphrase": "$PASSPHRASE",
  "deployedAt": "$DEPLOYED_AT",
  "commit": "$COMMIT",
  "contracts": {
    "programme": "$CONTRACT_ID"
  }
}
EOF

echo "Wrote $OUT"
