#!/bin/sh
set -eu
workflow=/opt/tekmerion/workflows/chainlink-appeals
runtime="$HOME/runtime"
command=${1:-listen}
if [ "$#" -gt 0 ]; then shift; fi

case "$command" in
  login)
    # CRE's callback binds container loopback. Publish only this forwarder.
    socat TCP-LISTEN:53683,bind=0.0.0.0,reuseaddr,fork TCP4:127.0.0.1:53682 &
    proxy=$!
    trap 'kill "$proxy" 2>/dev/null || true' EXIT HUP INT TERM
    cre login "$@"
    ;;
  status)
    exec cre whoami --non-interactive "$@"
    ;;
  check)
    cd "$workflow"
    npm test
    WORKFLOW_WASM="$workflow/chainlink-appeals.wasm" bun test tests/wasm/runtime.test.ts
    ;;
  config)
    exec node "$workflow/tools/config.mjs" /run/cre/config.json
    ;;
  listen|simulate)
    if [ -z "${CRE_SOLANA_PRIVATE_KEY:-}" ]; then
      echo 'Set CRE_SOLANA_PRIVATE_KEY to a dedicated test keypair in the workflow .env.' >&2
      exit 1
    fi
    node "$workflow/docker/configure.mjs" /run/cre/config.json "$runtime"
    cd "$runtime"
    if [ "$command" = listen ]; then
      if [ -z "${CRE_HTTP_PAYLOAD:-}" ]; then
        echo 'Set CRE_HTTP_PAYLOAD to the first Case JSON; CRE 1.39.0 requires an initial payload in non-interactive listen mode.' >&2
        exit 1
      fi
      set -- --listen --http-trigger-port 2000 --trigger-index 0 --http-payload "$CRE_HTTP_PAYLOAD" "$@"
    fi
    case "${CRE_SOLANA_RPC_URL:-}" in
      http://host.docker.internal:*) set -- --allow-insecure-rpc "$@" ;;
    esac
    exec cre workflow simulate . --non-interactive --target staging-settings \
      --env "$runtime/.env" --wasm "$workflow/chainlink-appeals.wasm" "$@"
    ;;
  *)
    echo 'Usage: listen | simulate [CRE flags] | login | status | config | check' >&2
    exit 2
    ;;
esac
