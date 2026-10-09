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
source "$project_root/scripts/build-tools.sh"
command -v node >/dev/null
command -v npm >/dev/null
export BPF_OUT_DIR="$deploy_dir"
# Quiet repeated local transport warnings; explicit user targets still win.
export RUST_LOG="tarpc=error,${RUST_LOG:-error}"

prepare_tools
restore_dependencies programs "$project_root/Cargo.toml"
restore_dependencies sdk "$sdk_manifest"
restore_dependencies consumer "$fixture_manifest"
restore_dependencies runtime "$runtime_manifest"
npm ci --prefix "$typescript_dir" --prefer-offline --no-audit --no-fund

export CARGO_NET_OFFLINE=true
host programs rustc --version
anchor --version
host programs cargo fmt --all -- --check
host sdk cargo fmt --manifest-path "$sdk_manifest" --all -- --check
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
for program in tekmerion sdk_consumer; do
  manifest="$project_root/programs/tekmerion/Cargo.toml"
  if [[ "$program" == sdk_consumer ]]; then manifest="$fixture_manifest"; fi
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
mkdir -p "$output_dir/idl"
host programs anchor idl build --program-name tekmerion --out "$output_dir/idl/tekmerion.json" \
  -- "${program_features[@]}" --locked --offline
python3 - "$output_dir/idl" "$update_idl" <<'PY'
import json
from pathlib import Path
import sys

generated = Path(sys.argv[1])
update = sys.argv[2] == "true"
actual = json.loads((generated / "tekmerion.json").read_text())
expected_path = Path("idl/tekmerion.json")
if update:
    expected_path.parent.mkdir(parents=True, exist_ok=True)
    expected_path.write_text(json.dumps(actual, indent=2) + "\n")
    print(f"Updated {expected_path}; review the interface diff.")
elif not expected_path.exists() or json.loads(expected_path.read_text()) != actual:
    raise SystemExit(f"IDL drift: {expected_path}; regenerate with --update-idl and review.")
else:
    print(f"Verified generated interface: {expected_path}")
PY

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
npm run format:check --prefix "$typescript_dir"
npm test --prefix "$typescript_dir"
echo 'All local contract and SDK tests passed.'
