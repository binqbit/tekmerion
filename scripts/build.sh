#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
project_root="$PWD"
if (( $# )); then
  case "$1" in
    --help|-h)
      echo 'Usage: bash scripts/build.sh'
      echo 'Build the mainnet SBF release and verify its generated IDL.'
      echo 'Output: target/production/release/{tekmerion.so,tekmerion.json,build-info.json,SHA256SUMS}'
      echo 'Prepares pinned tools and locked dependencies; requires no Node.js, RPC or wallet.'
      echo 'Run bash scripts/test.sh separately before deploying a release.'
      exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
fi

output_dir="$project_root/target/production"
source "$project_root/scripts/build-tools.sh"
program_features=(--no-default-features --features mainnet)
export CARGO_TERM_COLOR=never
export CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=true

prepare_tools
restore_dependencies programs "$project_root/Cargo.toml"
export CARGO_NET_OFFLINE=true

# A fresh output avoids reusing stripped binaries by modification time. The
# builder's disposable generated keypair stays here and is never packaged.
stage="$(mktemp -d "$output_dir/.build.XXXXXX")"
trap 'rm -rf -- "$stage"' EXIT
mkdir -p "$stage/deploy" "$stage/release"
build_log="$output_dir/build.log"

# cargo-build-sbf selects Cargo's release profile unless --debug is requested.
sbf program --manifest-path "$project_root/programs/tekmerion/Cargo.toml" \
  --sbf-out-dir "$stage/deploy" --tools-version "$sbf_tools_version" --arch "$sbf_arch" \
  "${program_features[@]}" --offline --skip-tools-install --jobs "$CARGO_BUILD_JOBS" \
  -- --locked --offline 2>&1 | tee "$build_log"

python3 - "$build_log" <<'PY'
from pathlib import Path
import re
import sys

log = re.sub(r"\x1b\[[0-9;]*m", "", Path(sys.argv[1]).read_text())
errors = [line for line in log.splitlines() if line.lstrip().startswith("Error:")
          or "undefined and not known syscalls" in line
          or "Calling them will trigger a run-time error" in line]
if errors:
    raise SystemExit("SBF diagnostics rejected:\n" + "\n".join(errors))
PY

host programs anchor idl build --program-name tekmerion --out "$stage/tekmerion.json" \
  -- "${program_features[@]}" --locked --offline 2>&1 | tee "$output_dir/idl.log"

host programs rustc --version --verbose > "$stage/host-rustc.txt"
anchor --version > "$stage/anchor.txt"
cargo-build-sbf --version > "$stage/builder.txt"
sbf_rust_version="$(python3 - "$stage/builder.txt" <<'PY'
from pathlib import Path
import sys

lines = Path(sys.argv[1]).read_text().splitlines()
print(next(line.split()[1] for line in lines if line.startswith("rustc ")))
PY
)"
sbf_rustc="$(rustup which --toolchain "$sbf_rust_version-sbpf-solana-$sbf_tools_version" rustc)"
"$sbf_rustc" --version --verbose > "$stage/sbf-rustc.txt"

python3 - "$project_root" "$stage" "$sbf_tools_version" "$sbf_arch" <<'PY'
from pathlib import Path
import hashlib
import json
import shutil
import struct
import subprocess
import sys
import tomllib

root, stage = map(Path, sys.argv[1:3])
bundle = stage / "release"
idl = json.loads((stage / "tekmerion.json").read_text())
expected_idl = json.loads((root / "idl/tekmerion.json").read_text())
if idl != expected_idl:
    raise SystemExit("IDL drift: run bash scripts/test.sh --update-idl and review the change.")

anchor = tomllib.loads((root / "Anchor.toml").read_text())
if idl["address"] != anchor["programs"]["localnet"]["tekmerion"]:
    raise SystemExit("Program identity differs between the generated IDL and Anchor.toml.")
mints = {account["address"] for instruction in idl["instructions"]
         for account in instruction["accounts"] if account["name"] == "mint"}
if mints != {"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"}:
    raise SystemExit("Generated IDL does not select the mainnet USDC mint.")

binary = stage / "deploy/tekmerion.so"
header = binary.read_bytes()[:64]
if (len(header) != 64 or header[:7] != b"\x7fELF\x02\x01\x01"
        or struct.unpack_from("<HH", header, 16) != (3, 263)
        or struct.unpack_from("<I", header, 48)[0] != 0):
    raise SystemExit("Expected an ELF64 little-endian SBF v0 shared object.")

shutil.copyfile(binary, bundle / "tekmerion.so")
(bundle / "tekmerion.json").write_text(json.dumps(idl, indent=2) + "\n")

def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

inputs = ["Cargo.toml", "Cargo.lock", "Anchor.toml", "rust-toolchain.toml",
          "programs/tekmerion/Cargo.toml", "idl/tekmerion.json",
          "scripts/build.sh", "scripts/build-tools.sh", "scripts/sbf-cargo.sh"]
inputs += [str(path.relative_to(root)) for path in (root / "programs/tekmerion/src").rglob("*.rs")]
source_files = {name: sha256(root / name) for name in sorted(inputs)}
source_digest = hashlib.sha256(json.dumps(source_files, sort_keys=True).encode()).hexdigest()

def git_output(*args):
    try:
        result = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True)
        return result.stdout.strip() if result.returncode == 0 else None
    except FileNotFoundError:
        return None

status = git_output("status", "--porcelain")
info = {
    "program": {"name": "tekmerion", "id": idl["address"], "network": "mainnet",
                "mint": next(iter(mints))},
    "build": {"profile": "release", "features": ["mainnet"], "default_features": False,
              "overflow_checks": True, "sbf_arch": sys.argv[4],
              "platform_tools": sys.argv[3], "locked": True, "offline_compilation": True},
    "tools": {name: (stage / file).read_text().strip() for name, file in {
        "host_rustc": "host-rustc.txt", "sbf_rustc": "sbf-rustc.txt",
        "anchor": "anchor.txt", "cargo_build_sbf": "builder.txt"}.items()},
    "git": {"revision": git_output("rev-parse", "HEAD"),
            "dirty": None if status is None else bool(status)},
    "source_sha256": source_digest,
    "source_files": source_files,
    "artifacts": {name: sha256(bundle / name) for name in ["tekmerion.so", "tekmerion.json"]},
}
(bundle / "build-info.json").write_text(json.dumps(info, indent=2) + "\n")
(bundle / "SHA256SUMS").write_text("".join(
    f"{sha256(bundle / name)}  {name}\n"
    for name in ["tekmerion.so", "tekmerion.json", "build-info.json"]
))

# Replace only these generated artifacts; never copy or replace deployment keys.
release = root / "target/production/release"
release.mkdir(parents=True, exist_ok=True)
for name in ["tekmerion.so", "tekmerion.json", "build-info.json", "SHA256SUMS"]:
    (bundle / name).replace(release / name)
print(f"Production build ready: {release}")
print(f"Program ID: {idl['address']}")
print(f"SBF SHA-256: {info['artifacts']['tekmerion.so']}")
PY
