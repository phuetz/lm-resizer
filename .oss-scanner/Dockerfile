# Construit LM Resizer depuis son dépôt, avec la chaîne Rust du projet (rust-toolchain.toml), puis lance les tests.
FROM rust:1-bookworm
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates git python3 bash coreutils procps \
    && rm -rf /var/lib/apt/lists/*

# contrat du scanner : la copie du dépôt vit dans l'image, sous /src
COPY . /src
WORKDIR /src

# La chaîne déclarée dans rust-toolchain.toml s'installe pendant cette phase en ligne ; les dépendances aussi.
RUN rustup show && cargo fetch --locked
ENV RUSTFLAGS="-C force-frame-pointers=yes" CARGO_INCREMENTAL=0
RUN cargo build --workspace --locked --all-targets
RUN cargo build --release --locked && install -m 0755 target/release/lm-resizer /usr/local/bin/lm-resizer

# Tests rapides pour vérifier l'image, sans faire échouer la construction.
RUN (cargo test --workspace --locked --no-fail-fast -- --test-threads=4 || echo "WARNING: cargo test reported failures")
