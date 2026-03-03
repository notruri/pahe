# Pahe CLI - Docker

# -------- initializatiion --------- #

FROM rust:1-slim-trixie AS base

LABEL org.opencontainers.image.source=https://github.com/notruri/pahe

RUN apt-get update -y \
    && apt-get install -y \
       pkg-config libssl-dev musl-tools \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add x86_64-unknown-linux-musl

# --------- preparation ----------- #

FROM base AS chef

RUN cargo install cargo-chef --locked

# ---------- planning ------------ #

FROM chef AS plan

WORKDIR /chef

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# ---------- cooking ------------ #

FROM chef AS cook

WORKDIR /kitchen

COPY --from=plan /chef/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --workspace --release --target x86_64-unknown-linux-musl

# ---------- runtime ------------ #

FROM alpine:latest

WORKDIR /app

COPY --from=cook /kitchen/target/x86_64-unknown-linux-musl/release/pahe-cli /app/pahe-cli

ENTRYPOINT [ "./pahe-cli" ]
