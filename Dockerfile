# syntax=docker/dockerfile:1
#
# seekrit-sdk-server — the cluster-side resolver that holds a service token,
# decrypts secrets locally (zero-knowledge, via crates/seekrit-core), and serves
# them over a tiny authed HTTP API for the External Secrets Operator's webhook
# provider. Multi-stage: build a fully-static musl binary, ship it on `scratch`.
# rustls + webpki-roots (all on `ring`, no aws-lc/OpenSSL) means no system CA
# store is needed, so the runtime image is just the binary (no OS, no shell) — a
# small attack surface for a sidecar that holds decrypted secrets in memory.
#
# It shares its zero-knowledge crypto with apps/run and apps/proxy via the
# crates/seekrit-core path dependency, which lives outside this build context.
# It is supplied as a named build context (`seekrit_core`) so the primary context
# stays apps/seekrit-sdk-server — see build-sdk-server-image.yml. To build locally
# from the repo root:
#
#     docker build -f apps/seekrit-sdk-server/Dockerfile \
#       --build-context seekrit_core=crates/seekrit-core \
#       -t seekritdev/sdk-server apps/seekrit-sdk-server
#
# Run it (pass the token + API key; bind 0.0.0.0 for a standalone container):
#
#     docker run --rm \
#       -e SEEKRIT_TOKEN=skt_… -e SEEKRIT_SDK_API_KEY=… \
#       -p 8080:8080 seekritdev/sdk-server

# ---- build stage: static musl binary ----------------------------------------
FROM rust:1.86-alpine AS build

# musl-dev + a C toolchain: needed to build `ring` (rustls' crypto backend).
RUN apk add --no-cache musl-dev gcc make

# Recreate the repo layout so the `../../crates/seekrit-core` path dependency in
# apps/seekrit-sdk-server/Cargo.toml resolves. The shared crate arrives via the
# named context.
WORKDIR /build/apps/seekrit-sdk-server
# Copy only what the binary build needs (tests/ is not compiled by `cargo build`).
# --locked pins the committed dependency set for a reproducible build.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY --from=seekrit_core Cargo.toml /build/crates/seekrit-core/Cargo.toml
COPY --from=seekrit_core src /build/crates/seekrit-core/src
COPY --from=seekrit_telemetry Cargo.toml /build/crates/seekrit-telemetry/Cargo.toml
COPY --from=seekrit_telemetry src /build/crates/seekrit-telemetry/src
RUN cargo build --release --locked --bin seekrit-sdk-server

# ---- runtime stage: nothing but the binary ----------------------------------
FROM scratch AS runtime
COPY --from=build /build/apps/seekrit-sdk-server/target/release/seekrit-sdk-server /seekrit-sdk-server

# Default API port — documentation only; the effective bind comes from --listen /
# SEEKRIT_SDK_LISTEN. The token comes from SEEKRIT_TOKEN and the endpoint's bearer
# key from SEEKRIT_SDK_API_KEY.
EXPOSE 8080
ENTRYPOINT ["/seekrit-sdk-server"]
