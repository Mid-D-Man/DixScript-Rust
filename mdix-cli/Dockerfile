# mdix-cli container image.
#
# Build context must be the REPO ROOT, not mdix-cli/ itself:
#   docker build -f mdix-cli/Dockerfile -t mdix-cli .
# mdix-cli is a member of this Cargo workspace (see the root Cargo.toml),
# and Cargo needs every workspace member's Cargo.toml present to even parse
# the workspace -- same reason CI's `cargo build -p mdix-cli` steps already
# run from the repo root rather than from inside mdix-cli/. (mdix-cli's own
# dixscript dependency is now a plain crates.io version, not a path dep --
# see mdix-cli/Cargo.toml -- but the workspace-parsing requirement still
# applies regardless.) See .dockerignore at the repo root for what's
# excluded from the build context.
#
# ── Cargo.lock prerequisite ─────────────────────────────────────────────
# The `--locked` build below will fail with "the lock file needs to be
# updated but --locked was passed" until Cargo.lock has been regenerated
# against mdix-cli's current Cargo.toml (its dixscript dependency changed
# from a path+version dep to a plain version dep, which is a different
# Cargo source kind even though the resolved version is unchanged -- the
# committed Cargo.lock still only has a path-sourced dixscript entry).
# Run the "Sync Cargo.lock" workflow (workflow_dispatch) once first if
# this hasn't been done since that Cargo.toml change landed.

# ── Build stage ──────────────────────────────────────────────────────────
# `rust:1-bookworm` (not `-slim`) is used deliberately: it ships gcc and
# the rest of build-essential already, which this dependency tree needs at
# build time even though the resulting binary is pure Rust at runtime --
#   - dixscript's default `bzip2-support` feature compiles a vendored C
#     bzip2 source tree via the `cc` crate.
#   - reqwest's `rustls-tls` backend (dixscript's `cloud-import` feature,
#     also on by default) still needs a C compiler for its crypto
#     backend's build script, even though it never links OpenSSL.
# The floating `1-bookworm` tag tracks the latest stable Rust 1.x release,
# matching the "always current stable" policy every CI workflow already
# uses via dtolnay/rust-toolchain@stable.
FROM rust:1-bookworm AS builder

WORKDIR /build
COPY . .

RUN cargo build -p mdix-cli --release --locked

# ── Runtime stage ────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# ca-certificates: dixscript's cloud-import feature (on by default) makes
# HTTPS requests via reqwest -- without this, every cloud-import call
# fails the TLS handshake with "unable to get local issuer certificate".
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/mdix /usr/local/bin/mdix

ENTRYPOINT ["mdix"]
CMD ["--help"]
