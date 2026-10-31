#!/usr/bin/env bash
# Sourced by test.sh. Local oracle fixtures exercise the normal program artifacts.
for network in mainnet devnet; do
  confidential_deploy="$output_dir/deploy-confidential-$network"
  production_deploy="$deploy_dir"
  if [[ "$network" == devnet ]]; then production_deploy="$output_dir/deploy-devnet"; fi
  mkdir -p "$confidential_deploy"
  cp "$production_deploy/tekmerion.so" "$production_deploy/keystone_forwarder.so" "$confidential_deploy/"
  cp "$production_deploy/chainlink_auditor.so" "$confidential_deploy/"
  host runtime cargo clippy --manifest-path "$runtime_manifest" \
    --all-targets --no-default-features --features "$network,local-fixtures" --locked --offline -- -D warnings
  BPF_OUT_DIR="$confidential_deploy" host runtime cargo test --manifest-path "$runtime_manifest" \
    --test contract confidential --no-default-features --features "$network,local-fixtures" --locked --offline -- --test-threads=2
  generated="$output_dir/confidential-$network.json"
  BPF_OUT_DIR="$confidential_deploy" host runtime cargo run --manifest-path "$runtime_manifest" \
    --example confidential_fixtures --no-default-features --features "$network,local-fixtures" --locked --offline \
    > "$generated" 2> "$output_dir/confidential-fixtures-$network.log"
  python3 - "$network" "$generated" "$update_fixtures" <<'PY'
import json, sys
from pathlib import Path
network, generated, update = sys.argv[1:]
actual = json.loads(Path(generated).read_text())
path = Path(f"sdk/fixtures/chainlink/confidential-{network}.json")
if update == "true":
    path.write_text(json.dumps(actual, indent=2) + "\n")
    print(f"Updated {path}; review the diff.")
elif not path.exists() or json.loads(path.read_text()) != actual:
    raise SystemExit(f"Confidential fixture drift: {path}; regenerate with --update-fixtures and review.")
else:
    print(f"Verified {path}")
PY
done
