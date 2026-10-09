#!/usr/bin/env bash
# Shared compiler/tool setup. Callers choose their own build output directory.

: "${project_root:?Set project_root before sourcing build-tools.sh}"
: "${output_dir:?Set output_dir before sourcing build-tools.sh}"
python3 -c 'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ required"'
host_toolchain="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["toolchain"]["channel"])' "$project_root/rust-toolchain.toml")"
sbf_tools_version=v1.53
sbf_arch=v0
build_tools_dir="$project_root/target/tools"
# Reuse previously installed local-test tools without moving existing artifacts.
export PATH="$build_tools_dir/bin:$project_root/target/local-contracts/tools/bin:$PATH"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"

host() (
  build_scope="$1"
  shift
  selected_cargo="$(rustup which --toolchain "$host_toolchain" cargo)"
  selected_bin="$(dirname "$selected_cargo")"
  export PATH="$selected_bin:$PATH"
  export RUSTC="$selected_bin/rustc"
  export RUSTDOC="$selected_bin/rustdoc"
  # Independently locked workspaces must not overwrite each other's un-hashed
  # cdylib/lib artifacts (notably Tekmerion's CPI library during IDL generation).
  export CARGO_TARGET_DIR="$output_dir/host/$build_scope"
  # IDL generation inspects this variable and may otherwise add a rustup-only
  # +toolchain argument to the explicitly selected real Cargo binary.
  unset RUSTUP_TOOLCHAIN
  "$@"
)

has_anchor() {
  command -v anchor >/dev/null && [[ "$(anchor --version)" == 'anchor-cli 1.2.1' ]]
}

has_sbf_builder() {
  command -v cargo-build-sbf >/dev/null || return 1
  local builder_version
  builder_version="$(cargo-build-sbf --version)"
  [[ "${builder_version%%$'\n'*}" == 'cargo-build-sbf 4.0.0' ]]
}

sbf() (
  build_scope="$1"
  shift
  rustup which --toolchain "$host_toolchain" cargo >/dev/null
  mkdir -p "$output_dir/sbf-cargo"
  ln -sf "$project_root/scripts/sbf-cargo.sh" "$output_dir/sbf-cargo/cargo"
  export PATH="$output_dir/sbf-cargo:$PATH"
  export CARGO="$output_dir/sbf-cargo/cargo"
  export RUSTUP_TOOLCHAIN="$host_toolchain"
  # Each independently locked workspace also needs its own SBF artifacts:
  # the deployed library and a caller's CPI library have different features.
  export CARGO_TARGET_DIR="$output_dir/sbf/$build_scope"
  env -u RUSTC -u RUSTDOC cargo-build-sbf "$@"
)

restore_dependencies() {
  local scope="$1" manifest="$2"
  if ! host "$scope" cargo fetch --manifest-path "$manifest" --locked --offline \
    >"$output_dir/dependencies-$scope.log" 2>&1; then
    host "$scope" cargo fetch --manifest-path "$manifest" --locked
  fi
}

prepare_tools() {
  command -v rustup >/dev/null
  export CARGO_NET_OFFLINE=false
  mkdir -p "$output_dir"

  # Restore the pinned compiler only when it or its checking components are absent.
  if ! rustup which --toolchain "$host_toolchain" cargo >/dev/null 2>&1; then
    rustup toolchain install "$host_toolchain" --profile minimal --component rustfmt --component clippy
  fi
  local installed_components
  installed_components="$(rustup component list --toolchain "$host_toolchain" --installed)"
  if [[ "$installed_components" != *rustfmt* || "$installed_components" != *clippy* ]]; then
    rustup component add --toolchain "$host_toolchain" rustfmt clippy
  fi
  if ! has_anchor; then
    host tools cargo install --locked --version 1.2.1 --root "$build_tools_dir" anchor-cli
  fi
  if ! has_sbf_builder; then
    host tools cargo install --locked --version 4.0.0 --root "$build_tools_dir" cargo-build-sbf
  fi

  # install-only reuses the pinned platform tools when already cached.
  sbf tools --install-only --tools-version "$sbf_tools_version"
}
