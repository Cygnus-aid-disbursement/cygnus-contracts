#!/usr/bin/env bash
# Run a whole programme cycle on Testnet: create, fund, claim, approve, disburse
# a batch of 50 synthetic beneficiaries, verify one inclusion proof, and refund
# the remainder. This is the proof of life for the contract.
#
# It reads the deployed contract id from deployments/testnet.json. Run
# scripts/deploy_testnet.sh first.
set -euo pipefail

NETWORK="${NETWORK:-testnet}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DEPLOYMENTS="${DEPLOYMENTS:-deployments/testnet.json}"
CID="$(node -e "console.log(require('./$DEPLOYMENTS').contracts.programme)")"
ASSET="$(stellar contract id asset --asset native --network "$NETWORK")"

# Milestone due dates are set in the past so the demo can reach refund_remainder
# in one run. That makes the claim land late, which the contract flags rather
# than blocks; the output below shows claimed_late = true.
NOW="$(date +%s)"
DUE=$((NOW - 3600))

H1="1111111111111111111111111111111111111111111111111111111111111111"
H2="2222222222222222222222222222222222222222222222222222222222222222"
META="9999999999999999999999999999999999999999999999999999999999999999"
EVIDENCE="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

ensure_key() {
  local name="$1"
  if ! stellar keys address "$name" >/dev/null 2>&1; then
    echo "Generating and funding $name..."
    stellar keys generate "$name" --network "$NETWORK" --fund >/dev/null
  fi
}

ensure_key cygnus-sponsor
ensure_key cygnus-implementer
ensure_key cygnus-approver

SPONSOR="$(stellar keys address cygnus-sponsor)"
IMPLEMENTER="$(stellar keys address cygnus-implementer)"
APPROVER="$(stellar keys address cygnus-approver)"

invoke() {
  # invoke <source> <fn> [args...]
  local source="$1"; shift
  local fn="$1"; shift
  stellar contract invoke --id "$CID" --source "$source" --network "$NETWORK" -- "$fn" "$@"
}

echo "== Cygnus demo =="
echo "contract:    $CID"
echo "asset:       $ASSET (native)"
echo "sponsor:     $SPONSOR"
echo "implementer: $IMPLEMENTER"
echo "approver:    $APPROVER"
echo

echo "1. create_programme (total 1000, two milestones 600 + 400)"
PID="$(invoke cygnus-sponsor create_programme \
  --sponsor "$SPONSOR" --implementer "$IMPLEMENTER" --asset "$ASSET" \
  --total 1000 \
  --milestones "[{\"description_hash\":\"$H1\",\"amount\":\"600\",\"due_by\":$DUE},{\"description_hash\":\"$H2\",\"amount\":\"400\",\"due_by\":$DUE}]" \
  --approver "{\"Single\":\"$APPROVER\"}" \
  --metadata_hash "$META")"
PID="$(echo "$PID" | tr -d '"')"
echo "   programme id: $PID"
echo

echo "2. fund (1000)"
invoke cygnus-sponsor fund --programme_id "$PID" --amount 1000
echo

echo "3. claim milestone 0 (implementer)"
invoke cygnus-implementer claim --programme_id "$PID" --milestone_index 0 --evidence_hash "$EVIDENCE"
echo

echo "4. approve milestone 0 (approver, releases 600)"
invoke cygnus-approver approve --programme_id "$PID" --milestone_index 0
echo

echo "5. build a 50-beneficiary batch (600 total, 12 each) and disburse"
BATCH="$(node scripts/merkle_root.mjs 50 12 0)"
BROOT="$(echo "$BATCH" | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>process.stdout.write(JSON.parse(d).root))")"
BTOTAL="$(echo "$BATCH" | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>process.stdout.write(JSON.parse(d).total))")"
BCOUNT="$(echo "$BATCH" | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>process.stdout.write(String(JSON.parse(d).count)))")"
echo "   root:  $BROOT"
echo "   total: $BTOTAL  count: $BCOUNT"
BINDEX="$(invoke cygnus-implementer disburse --programme_id "$PID" --batch_root "$BROOT" --total "$BTOTAL" --count "$BCOUNT")"
BINDEX="$(echo "$BINDEX" | tr -d '"')"
echo "   batch index: $BINDEX"
echo

echo "6. verify one beneficiary's inclusion proof (must be true)"
LEAF="$(echo "$BATCH" | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>process.stdout.write(JSON.parse(d).leaf))")"
PROOF="$(echo "$BATCH" | node -e "let d='';process.stdin.on('data',c=>d+=c).on('end',()=>process.stdout.write(JSON.stringify(JSON.parse(d).proof)))")"
RESULT="$(invoke cygnus-deployer verify_inclusion --programme_id "$PID" --batch_index "$BINDEX" --leaf "$LEAF" --proof "$PROOF")"
echo "   verify_inclusion: $RESULT"
if [ "$RESULT" != "true" ]; then
  echo "   FAILED: inclusion proof did not verify" >&2
  exit 1
fi
echo

echo "7. refund_remainder (sponsor recovers 400)"
REFUND="$(invoke cygnus-sponsor refund_remainder --programme_id "$PID")"
echo "   refunded: $REFUND"
echo

echo "== Demo complete for programme $PID =="
