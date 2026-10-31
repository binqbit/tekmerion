# Local CRE with Docker

This image builds the actual Tekmerion workflow and runs it with the official
CRE CLI. The host needs Docker Engine with Compose 2.24 or newer and a browser
for CRE login. Node, Bun, Javy and CRE are installed inside the image. Both
`linux/amd64` and `linux/arm64` release assets are configured.

Commands below start in `workflows/chainlink-appeals/`.

For service-driven Devnet execution, the Market repository provides
`compose.chainlink-devnet.yaml`. Its authenticated gateway persists admissions
and runs one broadcast simulation at a time. It does not use the rate-limited
manual listener described below. The `enrollment MAX_ATTEMPTS` and `release`
commands print the matching Profile configuration and a read-only deployment
manifest. Both use the official Devnet mock and CLI simulator identity.
The release helper takes its integrating program ID from `CRE_CONSUMER_PROGRAM`;
the Market overlay supplies Manteon's ID without coupling the workflow to it.

### Service boundaries

`src/` remains the deployable WASM workflow. It contains no local queue, Docker
process management or simulation verdict path. The local Node tools have these
separate responsibilities:

| Module under `tools/simulator/` | Responsibility |
| --- | --- |
| `auth.mjs` / `server.mjs` | Authenticate the existing CRE request format and acknowledge admission |
| `queue.mjs` | Persist admissions, deduplicate request IDs and serialize executions |
| `process.mjs` | Bound and stop the CLI process group, including subprocesses |
| `devnet.mjs` | Inspect immutable enrollment and export deployment pins through read-only RPC |
| `run.mjs` | Validate startup prerequisites and connect these components |

This gateway is needed because the manual CLI listener can acknowledge requests
that its rate limit later skips. The backend needs a durable admission rather
than treating that acknowledgement as an execution. The queue does not decide
verdicts or retry AI evaluations; the workflow and on-chain Request enforce those
rules. A successful CLI exit is an execution status, never the Case's verdict.

Startup checks CRE authentication and required environment values before accepting
jobs. This does not verify model credentials, wallet funding or future service
availability. Storage failures stop admissions and make health checks fail;
repair storage before restarting. Queued jobs resume, whereas a job whose CLI
started is retained as interrupted after a restart. Shutdown cancels the whole
CLI process group before another job can run.

## Build and check the workflow

```sh
docker compose build
docker compose run --rm cre-tests
```

The build uses pinned Node/Bun images, npm lockfiles and checksum-verified CRE
CLI 1.39.0. It compiles `src/main.ts` into WASM. `cre-tests` then runs the workflow
tests and executes that WASM against recorded host responses, without network
access, credentials or a running blockchain. It does not run Rust/SBF tests;
those remain available through `bash scripts/test.sh` from the Tekmerion root.

## Sign in to CRE

```sh
docker compose run --rm --service-ports cre-login
```

Open the login URL printed by the CLI in your host browser. The container
forwards its loopback callback to `127.0.0.1:53682` on the host. The named
`cre-auth` volume retains credentials between runs. Keep that port free while
logging in. A CRE account and outbound access are still required; this is not
an offline replacement for Chainlink authentication.

An authorized `CRE_API_KEY` in the ignored workflow `.env` can be used instead
when the account supports it. The image contains neither credentials nor your
host's CRE settings. `docker compose down` keeps login state; `down --volumes`
removes it.

## Configure the audit and test wallet

```sh
cp config.example.json config.local.json
# Only if .env does not already exist:
test -e .env || cp .env.example .env
```

In `config.local.json`, set your Policy address and HTTP trigger signer address.
The signer is an EVM address used by CRE authentication, not a Solana wallet.
Keep the agent configuration identical to the immutable Profile commitment.
`archiveUrl` may remain empty. Set `TEKMERION_MODEL_KEY` in `.env` for actual
OpenAI calls; recorded tests do not need it. The CLI resolves configured secret
names before running a trigger, so CLI-only input tests also need a value for
this variable. Use a dummy value only with fixtures that cannot call OpenAI.

CRE's Solana simulator also requires a Solana keypair, even without broadcasting.
Set `CRE_SOLANA_PRIVATE_KEY` in `.env` to a **dedicated test wallet**: either the
64-byte JSON keypair on one line or its base58 encoding. A host file path does
not work inside the container. Compose loads this value at runtime; the Docker
build context excludes `.env`, local configuration and generated artifacts.

If you need a new test wallet and do not have Solana CLI, generate it inside the
image. Run the following only if `.env` does not already contain that variable.
It appends the generated key directly to the ignored file:

```sh
chmod 600 .env
docker compose run --rm -T --workdir /opt/tekmerion/sdk/typescript --entrypoint node cre-tests --input-type=module -e \
  'import {Keypair} from "@solana/web3.js"; process.stdout.write("\nCRE_SOLANA_PRIVATE_KEY=" + JSON.stringify([...Keypair.generate().secretKey]) + "\n");' \
  >> .env
```

Fund that wallet on the same test cluster if the simulation reaches transaction
execution. For read-only or input-rejection tests, no transaction is submitted.
Never use a production wallet for this environment.

The RPC defaults to `https://api.devnet.solana.com`. To use a validator already
running on your host, put this in `.env`:

```dotenv
CRE_SOLANA_RPC_URL=http://host.docker.internal:8899
```

The wrapper allows HTTP for this explicit Docker-to-host address. For another
container, use its Docker service name and explicitly pass `--allow-insecure-rpc`
if it exposes HTTP. This Compose file starts the CRE simulator only; it does not
start or deploy a Solana validator, Tekmerion or the Forwarder. The configured
network selects program identities and must match the accounts you prepared.

## Accept HTTP requests

CLI 1.39.0 requires an initial HTTP payload even in non-interactive listen mode.
Set the first real Case to process in `.env`:

```dotenv
CRE_HTTP_PAYLOAD={"case":"YOUR_FIRST_CASE_ADDRESS"}
```

The first Case runs when the service starts. Subsequent Cases arrive over HTTP:

```sh
docker compose run --rm cre config
docker compose up -d cre
docker compose logs -f cre
```

The first command validates the configuration and prints its agent hash. The
listener binds only `127.0.0.1:2000` on the host. Submit an actual Case address:

```sh
curl --fail-with-body http://localhost:2000/trigger \
  -H 'Content-Type: application/json' \
  -d '{"input":{"case":"YOUR_CASE_ADDRESS"}}'
```

The CLI's HTTP server unwraps `input`; a 200 response acknowledges queuing,
not a successful audit. Read the execution result in the container logs.
The default CLI limit permits one HTTP execution per 30 seconds. Requests sent
too soon can be acknowledged and then skipped with `Trigger rate limited` in
the log; wait for the limit window before submitting another Case.
Each executed request starts a fresh local run. The workflow accepts only the
Case address; it cannot override the model, Agreement, Result or verdict. A
syntactically invalid address or missing account should fail, not produce an
audit result. An execution error can stop the CLI listener; inspect the logs,
correct the input and restart it. Local CLI HTTP requests do not provide
production JWT guarantees.

For one execution instead of a listener, stop the service and run:

```sh
docker compose stop cre
docker compose run --rm cre simulate --trigger-index 0 \
  --http-payload '{"case":"YOUR_CASE_ADDRESS"}'
```

For one pending-request scan, use `simulate --trigger-index 1`. Listen mode is
HTTP-driven; it does not automatically schedule the cron handler. After code
changes, run `docker compose up -d --build cre`; restarting alone does not rebuild
the WASM. Stop the environment with `docker compose down`.

## What this proves

The service runs the official CLI and the same compiled workflow used by the
project. Its confidential handler is **simulated**: there is no hardware TEE or
independent DON on your computer. No `--broadcast` flag is added by default.
HTTP calls still happen, including paid model calls when execution reaches them.

A complete audit needs an existing Policy, Profile, Case and prepared Request
on the configured Solana RPC. The workflow verifies those accounts and their
commitments before calling the model. It also confirms execution claims on-chain
before research. A dry run cannot substitute for those confirmed state changes.

The CLI's Solana simulation uses its own mock Forwarder. A receiving Profile
must declare the corresponding program and state to accept its reports; a
Profile configured for another Forwarder rejects them. For a deterministic
local contract lifecycle, use `scripts/demo-confidential.sh` with its local
oracle fixtures and ordinary contract build. Neither path establishes live
DON or confidential execution guarantees.

The image and Compose files do not start an autonomous Chainlink network, deploy
contracts, request funds, or broadcast transactions. They provide a repeatable
development environment for CLI simulation and workflow checks.

References: [CRE simulation](https://docs.chain.link/cre/guides/operations/simulating-workflows),
[HTTP simulation](https://docs.chain.link/cre/guides/workflow/using-triggers/http-trigger/testing-in-simulation),
[Confidential access](https://docs.chain.link/cre/guides/operations/confidential-workflows-access),
[pinned CLI release](https://github.com/smartcontractkit/cre-cli/releases/tag/v1.39.0),
[Solana simulation key requirements](https://github.com/smartcontractkit/cre-cli/blob/v1.39.0/cmd/workflow/simulate/chain/solana/chaintype.go).
