# syntax=docker/dockerfile:1.7
# Native Docker and Podman build recipe.

# ── dependency cache layer (cargo-chef) ──────────────────────────────────────
FROM rust:1-bookworm AS chef
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libz3-dev \
        libgtk-3-dev \
        libwebkit2gtk-4.1-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        patchelf \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY kani-proofs ./kani-proofs
COPY vendor ./vendor
RUN cargo chef prepare --recipe-path recipe.json

# ── build ─────────────────────────────────────────────────────────────────────
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# vendor/ holds [patch.crates-io] path targets (not workspace members), so
# cargo-chef's recipe.json doesn't virtualize them the way it does for
# crates/xtask/kani-proofs — cargo still reads their real Cargo.toml off
# disk even during the synthetic `cook` step, so vendor/ must be present
# *before* cook runs, not just before the later real source COPY block.
COPY vendor ./vendor
RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
COPY kani-proofs ./kani-proofs
COPY docs ./docs
COPY rules ./rules
COPY scripts ./scripts
COPY windows ./windows

# The workspace test suite is available as an explicit build target. It is not
# part of the release-image path: main CI has already run it before this
# publish workflow starts, and retaining its debug artifacts exhausts runner
# storage while Podman snapshots intermediate layers.
FROM builder AS test
RUN cargo test --workspace --features 'audit,autoresearch,b00t,classification,core,default,events,full,hsm,legacy,legal-z3,llm,local-llm,mistralrs-llm,ontology,reconciliation,self-update,tax,xero'

FROM builder AS release
RUN cargo build -p ledgerr-mcp --release --bin ledgerr-mcp-server --features 'audit,b00t,classification,core,events,full,hsm,legacy,llm,ontology,reconciliation,self-update,tax,xero'

# ── runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app

COPY --from=release /app/target/release/ledgerr-mcp-server /usr/local/bin/ledgerr-mcp-server

ENV LEDGERR_WORKBOOK_PATH=/data/tax-ledger.xlsx
ENV LEDGER_PDF_INBOX=/data/inbox

CMD ["/usr/local/bin/ledgerr-mcp-server"]
