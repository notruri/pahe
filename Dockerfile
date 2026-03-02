# Pahe CLI - Docker

# -------- initializatiion --------- #

FROM rust:1-slim-trixie AS base

LABEL org.opencontainers.image.source=https://github.com/notruri/pahe

RUN apt-get update -y \
    && apt-get install -y \
       pkg-config libssl-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

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
RUN cargo build -p pahe-cli --release

# ---------- runtime ------------ #

FROM base AS runtime

WORKDIR /app

COPY --from=cook /kitchen/target/release/pahe-cli /app/pahe-cli

# ---------- launch ------------ #

FROM runtime AS launch

ENTRYPOINT [ "./pahe-cli" ]
