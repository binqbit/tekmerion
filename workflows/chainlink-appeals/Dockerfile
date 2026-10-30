# syntax=docker/dockerfile:1
FROM docker.io/oven/bun:1.3.13-debian@sha256:e95356cb8e1de62ad69ab3bd3584ba947013d27650a226804d2fc0af4e17dac2 AS bun
FROM docker.io/library/node:22-bookworm-slim@sha256:4b150b48b4116e2a22f2c39cc27025fbf4f153e2078fd3b14d159fdb5c7279da

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libstdc++6 socat unzip \
    && rm -rf /var/lib/apt/lists/*
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
COPY workflows/chainlink-appeals/docker/install-cre.sh /tmp/install-cre.sh
RUN sh /tmp/install-cre.sh && rm /tmp/install-cre.sh

WORKDIR /opt/tekmerion
RUN chown node:node . && mkdir -p /home/node/.cre /home/node/runtime \
    && chown -R node:node /home/node/.cre /home/node/runtime
ENV HOME=/home/node
USER node

COPY --chown=node:node sdk/typescript/ sdk/typescript/
RUN npm ci --prefix sdk/typescript && npm run build --prefix sdk/typescript
COPY --chown=node:node sdk/chainlink-typescript/ sdk/chainlink-typescript/
RUN npm ci --prefix sdk/chainlink-typescript && npm run build --prefix sdk/chainlink-typescript
COPY --chown=node:node sdk/fixtures/chainlink/ sdk/fixtures/chainlink/
COPY --chown=node:node tests/fixtures/confidential/ tests/fixtures/confidential/
COPY --chown=node:node workflows/chainlink-appeals/ workflows/chainlink-appeals/

WORKDIR /opt/tekmerion/workflows/chainlink-appeals
RUN npm ci && npm run build \
    && bun node_modules/@chainlink/cre-sdk/bin/cre-compile.ts src/main.ts chainlink-appeals.wasm

EXPOSE 2000 53683
ENTRYPOINT ["sh", "/opt/tekmerion/workflows/chainlink-appeals/docker/entrypoint.sh"]
CMD ["listen"]
