#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
project_root="$PWD"
output_dir="$project_root/target/local-contracts"
reuse=false
if [[ "${1:-}" == --reuse-build && $# == 1 ]]; then reuse=true
elif (( $# )); then echo 'Usage: bash scripts/demo-confidential.sh [--reuse-build]' >&2; exit 2; fi
if ! "$reuse"; then bash scripts/test.sh; fi
source scripts/build-tools.sh
export BPF_OUT_DIR="$output_dir/deploy-confidential-mainnet"
test -f "$BPF_OUT_DIR/chainlink_auditor.so"
export RUST_LOG=error
mkdir -p "$output_dir/confidential-demo"
npm run build --prefix sdk/typescript
npm run build --prefix sdk/chainlink-typescript
npm run build --prefix workflows/chainlink-appeals
CONFIDENTIAL_REPLAY_OUTPUT="$output_dir/confidential-demo/verdict.json" \
  node --test workflows/chainlink-appeals/tests/audit/execution.test.mjs
host runtime cargo run --manifest-path tests/Cargo.toml --example replay_confidential \
  --no-default-features --features mainnet,local-fixtures --locked --offline \
  -- "$output_dir/confidential-demo/verdict.json"
echo "Recorded agent verdict verified by Forwarder SBF and applied by core: $output_dir/confidential-demo/verdict.json"
echo 'Synthetic local DON/model evidence only; public confidential enrollment remains closed.'
