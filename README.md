# seekrit-sdk-server

A small, long-lived **cluster-side resolver** that lets the
[External Secrets Operator](https://external-secrets.io) (ESO) sync seekrit
secrets into Kubernetes — on **stock, unmodified ESO** — without the seekrit API
ever seeing plaintext.

This repo is a **read-only mirror**, published from seekrit's monorepo so this
service is auditable before you trust it with a service token inside your
cluster. Don't commit directly here — it'll be overwritten on the next sync.
Issues and PRs are welcome; accepted changes get ported upstream by a
maintainer.

seekrit is zero-knowledge: `GET /v1/resolve` returns only ciphertext plus a DEK
wrapped to the caller's public key, and decryption happens client-side. ESO,
which expects to *pull plaintext* from a provider, therefore can't talk to the
seekrit API directly. This service is the missing client: it holds a service
token, resolves + decrypts **locally** via [`vendor/seekrit-core`](vendor/seekrit-core)
(the same zero-knowledge path `seekrit-run` and `seekrit-proxy` use elsewhere in
seekrit), caches the result, and serves it over a tiny bearer-authed HTTP API
that ESO's built-in **webhook** provider reads.

```
  ExternalSecret ─▶ ESO (stock) ──GET /v1/secret/NAME──▶ seekrit-sdk-server ──GET /v1/resolve──▶ seekrit API
   (you write)      (unmodified)   Authorization: <key>   holds skt_ token,                     (ciphertext +
                                                           decrypts locally, caches,             wrapped DEK)
                                                           refreshes on a timer
```

Most users never run this directly — the
[`seekrit-eso` Helm chart](https://github.com/seekritdev/helm-charts/tree/main/charts/seekrit-eso)
deploys it and wires ESO for you. See the
[Kubernetes guide](https://seekrit.dev/docs/guides/kubernetes).

## How it works

1. **Startup (fail-closed).** Reads `SEEKRIT_TOKEN`, calls `GET /v1/resolve`, and
   decrypts every granted secret into memory. If the token is bad, the API is
   unreachable, or a layer won't decrypt, it refuses to start.
2. **Serving.** Answers three routes (below). Plaintext lives only in this
   process, in `Zeroizing` buffers scrubbed on drop.
3. **Refresh.** Re-resolves every `--refresh-interval` (default 60s) and swaps the
   snapshot atomically. A failed refresh is non-fatal — it keeps serving the
   last-good snapshot and logs a warning, so a transient API blip never breaks
   ESO syncs.
4. **Auth.** `/v1/secret*` require `Authorization: Bearer <api-key>` (constant-time
   compared). The service decrypts *everything* the token can reach, so it must
   not be an open in-cluster endpoint; the key is the second line of defense
   behind the Service's network reachability.

## API

| Method / path | Auth | Response |
| --- | --- | --- |
| `GET /v1/secret/<name>` | `Bearer <api-key>` | `200 {"value":"…"}` — or `404` if the name isn't in the resolved environment (lets ESO honor its `deletionPolicy`). |
| `GET /v1/secrets` | `Bearer <api-key>` | `200 {"data":{NAME:value,…}}` — the whole environment, for `target.template` pulls. |
| `GET /healthz` | none | `200` (liveness/readiness). |

ESO's webhook provider reads a single value with `result.jsonPath: "$.value"`.

## Usage

```sh
export SEEKRIT_TOKEN=skt_…
export SEEKRIT_SDK_API_KEY=…        # gates /v1/secret*
seekrit-sdk-server --listen 0.0.0.0:8080
```

### Options

| Flag | Default | Meaning |
| --- | --- | --- |
| `--listen <addr>` | `0.0.0.0:8080` (`SEEKRIT_SDK_LISTEN`) | Bind address. |
| `-t, --token <skt_…>` | `SEEKRIT_TOKEN` | Service token (required). |
| `--api-key <key>` | `SEEKRIT_SDK_API_KEY` | Bearer key for `/v1/secret*` (required). |
| `--api-url <url>` | `SEEKRIT_API_URL` or `https://api.seekrit.dev` | API base URL. |
| `--refresh-interval <d>` | `60s` (`SEEKRIT_SDK_REFRESH_INTERVAL`) | Re-resolve cadence: `30s`, `5m`, `1h`, or bare seconds. |

**One token = one environment.** A service token binds to a single app
environment (plus its composed group slices); that scope *is* this service's
blast radius. To serve multiple environments, run one instance per token.

## Container image

Published to Docker Hub as `seekritdev/sdk-server` (multi-arch, a single static
musl binary on `scratch` — no OS, no shell). `latest` + `<version>` are cut on
release; `edge` tracks `main`. Built and published from seekrit's monorepo CI,
not from this repo.

```sh
docker run --rm \
  -e SEEKRIT_TOKEN=skt_… -e SEEKRIT_SDK_API_KEY=… \
  -p 8080:8080 seekritdev/sdk-server
```

## Build

```sh
cargo build --release   # target/release/seekrit-sdk-server
cargo test              # unit + HTTP integration tests

# Container (the shared crate is a named build context):
docker build -f Dockerfile --build-context seekrit_core=vendor/seekrit-core \
  -t seekritdev/sdk-server .
```

## `vendor/seekrit-core`

Vendored, not developed here: it's [seekrit](https://seekrit.dev)'s shared,
transport-free zero-knowledge crate (service-token key recovery, DEK unwrap,
secret decryption), also used by `seekrit-run` and `seekrit-proxy` in the
private monorepo. Its canonical source lives there — changes to it arrive here
via the same mirror sync as everything else in this repo.
