# mdix-cli container image.
#
# Build context must be the REPO ROOT, not mdix-cli/ itself:
#   docker build -f mdix-cli/Dockerfile -t mdix-cli .
# mdix-cli depends on dixscript via a workspace path dependency (see
# mdix-cli/Cargo.toml), and Cargo needs every workspace member's Cargo.toml
# present to even parse the workspace -- same reason CI's
# `cargo build -p mdix-cli` steps already run from the repo root rather
# than from inside mdix-cli/. See .dockerignore at the repo root for what's
# excluded from the build context.

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
