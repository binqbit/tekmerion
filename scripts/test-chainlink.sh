#!/usr/bin/env bash
# Sourced by test.sh after mainnet SBF tests; shares its pinned toolchain/output.
for network in mainnet devnet; do
  chainlink_deploy="$deploy_dir"
  if [[ "$network" == devnet ]]; then
    chainlink_deploy="$output_dir/deploy-devnet"
    mkdir -p "$chainlink_deploy"
    cp "$deploy_dir/keystone_forwarder.so" "$chainlink_deploy/keystone_forwarder.so"
    for program in tekmerion chainlink-auditor; do
      log="$output_dir/sbf-$program-devnet.log"
      sbf "$program-devnet" --manifest-path "$project_root/programs/$program/Cargo.toml" \
        --sbf-out-dir "$chainlink_deploy" --tools-version "$sbf_tools_version" --arch "$sbf_arch" \
        --no-default-features --features devnet --offline --skip-tools-install \
        --jobs "$CARGO_BUILD_JOBS" -- --locked --offline > "$log" 2>&1
      python3 scripts/check-sbf-log.py "$log"
    done
  fi
  idl="$output_dir/idl/chainlink-$network.json"
  host programs anchor idl build --program-name chainlink_auditor --out "$idl" \
    -- --no-default-features --features "$network" --locked --offline
  python3 - "$network" "$idl" "$update_idl" <<'PY'
from pathlib import Path
import json
import sys
network, idl, update_idl = sys.argv[1:]
value = json.loads(Path(idl).read_text())
path = Path("idl/chainlink_auditor.json" if network == "mainnet" else "idl/devnet/chainlink_auditor.json")
if update_idl == "true":
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")
    print(f"Updated {path}; review the diff.")
elif not path.exists() or json.loads(path.read_text()) != value:
    raise SystemExit(f"Chainlink interface drift: {path}")
else:
    print(f"Verified {path}")
PY
done
