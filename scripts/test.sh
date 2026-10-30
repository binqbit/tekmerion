#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
project_root="$PWD"
update_idl=false
update_fixtures=false
while (( $# )); do
  case "$1" in
    --update-idl) update_idl=true ;;
    --update-fixtures) update_fixtures=true ;;
    --help|-h)
      echo 'Usage: bash scripts/test.sh [--update-idl] [--update-fixtures]'
      echo 'Build local SBF programs and run all contract, Rust SDK and TypeScript SDK tests.'
      echo 'Missing build tools and dependencies are restored automatically. Tests use no public RPC or wallet.'
      echo '--update-idl: replace maintained IDL snapshots with generated interfaces.'
      echo '--update-fixtures: replace maintained SDK vectors with local execution results.'
      exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
# The default contract mint is created in the disposable local bank. A Cargo
# identity feature selects account constants, never an execution endpoint.
program_features=(--no-default-features --features mainnet)

output_dir="$project_root/target/local-contracts"
deploy_dir="$output_dir/deploy"
runtime_manifest="$project_root/tests/Cargo.toml"
sdk_manifest="$project_root/sdk/rust/Cargo.toml"
fixture_manifest="$project_root/tests/programs/sdk_consumer/Cargo.toml"
typescript_dir="$project_root/sdk/typescript"
adapter_typescript_dir="$project_root/sdk/chainlink-typescript"
chainlink_sdk_manifest="$project_root/sdk/chainlink-rust/Cargo.toml"
forwarder_manifest="$project_root/tests/third-party/chainlink-solana/contracts/programs/keystone-forwarder/Cargo.toml"
workflow_dir="$project_root/workflows/chainlink-appeals"
source "$project_root/scripts/build-tools.sh"
command -v node >/dev/null
command -v npm >/dev/null
command -v bun >/dev/null
python3 scripts/verify-chainlink-source.py
export BPF_OUT_DIR="$deploy_dir"
# Quiet repeated local transport warnings; explicit user targets still win.
export RUST_LOG="tarpc=error,${RUST_LOG:-error}"

prepare_tools
restore_dependencies programs "$project_root/Cargo.toml"
restore_dependencies sdk "$sdk_manifest"
restore_dependencies consumer "$fixture_manifest"
restore_dependencies runtime "$runtime_manifest"
restore_dependencies chainlink-sdk "$chainlink_sdk_manifest"
restore_dependencies forwarder "$forwarder_manifest"
npm ci --prefix "$typescript_dir" --prefer-offline --no-audit --no-fund
npm ci --prefix "$adapter_typescript_dir" --prefer-offline --no-audit --no-fund
npm ci --prefix "$workflow_dir" --prefer-offline --no-audit --no-fund

export CARGO_NET_OFFLINE=true
host programs rustc --version
anchor --version
host programs cargo fmt --all -- --check
host sdk cargo fmt --manifest-path "$sdk_manifest" --all -- --check
host chainlink-sdk cargo fmt --manifest-path "$chainlink_sdk_manifest" --all -- --check
host chainlink-sdk cargo clippy --manifest-path "$chainlink_sdk_manifest" --all-targets "${program_features[@]}" --locked --offline -- -D warnings
host consumer cargo fmt --manifest-path "$fixture_manifest" --all -- --check
host runtime cargo fmt --manifest-path "$runtime_manifest" --all -- --check
host programs cargo clippy --workspace --all-targets "${program_features[@]}" --locked --offline -- -D warnings
host sdk cargo clippy --manifest-path "$sdk_manifest" --all-targets --no-default-features --features mainnet,host,cpi,rpc --locked --offline -- -D warnings
host sdk cargo check --manifest-path "$sdk_manifest" --lib --no-default-features --features mainnet,cpi --locked --offline
host consumer cargo clippy --manifest-path "$fixture_manifest" --all-targets "${program_features[@]}" --locked --offline -- -D warnings
host runtime cargo clippy --manifest-path "$runtime_manifest" --all-targets "${program_features[@]}" --locked --offline -- -D warnings
host programs cargo test --workspace "${program_features[@]}" --locked --offline
# Invalid feature combinations must not silently select a trust configuration.
for selection in neither both; do
  invalid_features=()
  if [[ "$selection" == both ]]; then invalid_features=(--features mainnet,devnet); fi
  log="$output_dir/features-$selection.log"
  if host programs cargo check -p tekmerion --no-default-features "${invalid_features[@]}" --locked --offline >"$log" 2>&1; then
    echo "Invalid network feature selection compiled: $selection" >&2
    exit 1
  fi
  python3 - "$log" <<'PY'
from pathlib import Path
import sys

if "Select exactly one network: mainnet or devnet" not in Path(sys.argv[1]).read_text():
    raise SystemExit("Feature rejection failed for an unrelated reason; inspect " + sys.argv[1])
PY
done
mkdir -p "$deploy_dir"
for program in tekmerion chainlink_auditor sdk_consumer; do
  manifest="$project_root/programs/tekmerion/Cargo.toml"
  if [[ "$program" == sdk_consumer ]]; then manifest="$fixture_manifest"; fi
  if [[ "$program" == chainlink_auditor ]]; then manifest="$project_root/programs/chainlink-auditor/Cargo.toml"; fi
  build_log="$output_dir/sbf-$program.log"
  sbf "$program" --manifest-path "$manifest" --sbf-out-dir "$deploy_dir" \
    --tools-version "$sbf_tools_version" --arch "$sbf_arch" "${program_features[@]}" \
    --offline --skip-tools-install --jobs "$CARGO_BUILD_JOBS" -- --locked --offline 2>&1 | tee "$build_log"
  # The SBF compiler can print stack-frame errors while Cargo still exits zero.
  python3 - "$build_log" <<'PY'
from pathlib import Path
import sys

errors = [line for line in Path(sys.argv[1]).read_text().splitlines() if line.startswith("Error:")]
if errors:
    raise SystemExit("SBF diagnostics rejected:\n" + "\n".join(errors))
PY
  test -f "$deploy_dir/$program.so"
done
sbf forwarder --manifest-path "$forwarder_manifest" --sbf-out-dir "$deploy_dir" \
  --tools-version "$sbf_tools_version" --arch "$sbf_arch" --offline --skip-tools-install \
  --jobs "$CARGO_BUILD_JOBS" -- --locked --offline > "$output_dir/sbf-forwarder.log" 2>&1
python3 scripts/check-sbf-log.py "$output_dir/sbf-forwarder.log"
mkdir -p "$output_dir/idl"
for network in mainnet devnet; do
  host programs anchor idl build --program-name tekmerion --out "$output_dir/idl/tekmerion-$network.json" \
    -- --no-default-features --features "$network" --locked --offline
  python3 - "$network" "$output_dir/idl/tekmerion-$network.json" "$update_idl" <<'PYIDL'
import json
from pathlib import Path
import sys

network, generated, update = sys.argv[1:]
actual = json.loads(Path(generated).read_text())
paths = [Path("idl/tekmerion.json" if network == "mainnet" else "idl/devnet/tekmerion.json"),
         Path(f"sdk/typescript/src/idl/{network}.json")]
for path in paths:
    if update == "true":
        path.write_text(json.dumps(actual, indent=2) + "\n")
        print(f"Updated {path}; review the interface diff.")
    elif not path.exists() or json.loads(path.read_text()) != actual:
        raise SystemExit(f"IDL drift: {path}; regenerate with --update-idl and review.")
    else:
        print(f"Verified generated interface: {path}")
PYIDL
done

host runtime cargo test --manifest-path "$runtime_manifest" "${program_features[@]}" --locked --offline -- --test-threads=2

if ! host runtime cargo run --manifest-path "$runtime_manifest" --example sdk_fixtures \
  "${program_features[@]}" --locked --offline > "$output_dir/sdk-fixtures.json" 2> "$output_dir/sdk-fixtures.log"; then
  cat "$output_dir/sdk-fixtures.log" >&2
  exit 1
fi
python3 - "$output_dir/sdk-fixtures.json" "$update_fixtures" <<'PY'
import json
from pathlib import Path
import sys

actual = json.loads(Path(sys.argv[1]).read_text())
expected = Path("sdk/fixtures/sdk-mainnet.json")
if sys.argv[2] == "true":
    expected.parent.mkdir(parents=True, exist_ok=True)
    expected.write_text(json.dumps(actual, indent=2) + "\n")
    print(f"Updated {expected}; review the protocol and quote changes.")
elif not expected.exists() or json.loads(expected.read_text()) != actual:
    raise SystemExit(f"SDK fixture drift: {expected}; regenerate with --update-fixtures and review.")
else:
    print(f"Verified runtime SDK vectors: {expected}")
PY

host sdk cargo test --manifest-path "$sdk_manifest" --no-default-features --features mainnet,host,cpi,rpc --locked --offline
source scripts/test-chainlink.sh
if ! BPF_OUT_DIR="$output_dir/deploy-devnet" host runtime cargo run --manifest-path "$runtime_manifest" \
  --example sdk_fixtures --no-default-features --features devnet --locked --offline \
  > "$output_dir/sdk-fixtures-devnet.json" 2> "$output_dir/sdk-fixtures-devnet.log"; then
  cat "$output_dir/sdk-fixtures-devnet.log" >&2
  exit 1
fi
python3 - "$output_dir/sdk-fixtures-devnet.json" "$update_fixtures" <<'PYFIXTURES'
import json
from pathlib import Path
import sys

actual = json.loads(Path(sys.argv[1]).read_text())
path = Path("sdk/fixtures/sdk-devnet.json")
if sys.argv[2] == "true":
    path.write_text(json.dumps(actual, indent=2) + "\n")
    print(f"Updated {path}; review the protocol and quote changes.")
elif not path.exists() or json.loads(path.read_text()) != actual:
    raise SystemExit(f"SDK fixture drift: {path}; regenerate with --update-fixtures and review.")
else:
    print(f"Verified runtime SDK vectors: {path}")
PYFIXTURES
source scripts/test-confidential.sh
npm run format:check --prefix "$typescript_dir"
npm test --prefix "$typescript_dir"
npm run format:check --prefix "$adapter_typescript_dir"
npm test --prefix "$adapter_typescript_dir"
npm test --prefix "$workflow_dir"
(cd "$workflow_dir" && bun node_modules/@chainlink/cre-sdk/bin/cre-compile.ts src/main.ts "$output_dir/chainlink-appeals.wasm")
(cd "$workflow_dir" && WORKFLOW_WASM="$output_dir/chainlink-appeals.wasm" \
  CONFIDENTIAL_REPLAY_OUTPUT="$output_dir/confidential-wasm-verdict.json" bun test tests/wasm/runtime.test.ts)
CONFIDENTIAL_REPLAY_OUTPUT="$output_dir/confidential-workflow-verdict.json" node --test "$workflow_dir/tests/audit/execution.test.mjs"
for verdict in confidential-workflow-verdict confidential-wasm-verdict; do
  BPF_OUT_DIR="$output_dir/deploy-confidential-mainnet" host runtime cargo run --manifest-path "$runtime_manifest" \
    --example replay_confidential --no-default-features --features mainnet,local-fixtures --locked --offline \
    -- "$output_dir/$verdict.json"
done
bash scripts/demo-confidential.sh --reuse-build
echo 'All local contract, SDK and Chainlink workflow tests passed.'
