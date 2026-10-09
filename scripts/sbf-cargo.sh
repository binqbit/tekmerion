#!/usr/bin/env bash
set -euo pipefail

# cargo-build-sbf invokes ordinary Cargo for metadata, then cargo +<SBF> for
# compilation. Route both to exact tools even without rustup proxies in PATH.
selected_toolchain="${RUSTUP_TOOLCHAIN:?SBF runner must select a host toolchain}"
if [[ "${1:-}" == +* ]]; then
  selected_toolchain="${1:1}"
  shift
fi
selected_cargo="$(rustup which --toolchain "$selected_toolchain" cargo)"
selected_bin="$(dirname "$selected_cargo")"
export PATH="$selected_bin:$PATH"
export RUSTC="$selected_bin/rustc"
export RUSTDOC="$selected_bin/rustdoc"
exec "$selected_cargo" "$@"
