# ---------- build ----------
FROM rust:1.98-slim-bookworm AS builder
# Built from the repo root (see compose.yaml): build.rs embeds ../edmc-plugin
# for the plugin download.
WORKDIR /app/backend

# Dependencies first, in their own cached layer. The placeholder build.rs keeps
# this layer independent of the plugin's files.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src \
 && echo 'fn main() {}' > src/main.rs \
 && echo 'fn main() {}' > build.rs \
 && cargo build --release \
 && rm -rf src build.rs

COPY edmc-plugin /app/edmc-plugin
COPY backend/build.rs ./
COPY backend/src ./src
COPY backend/migrations ./migrations
# The dev console page (served only with DEV_CONSOLE=1) is compiled in.
COPY backend/dev ./dev
RUN touch src/main.rs build.rs && cargo build --release

# ---------- runtime ----------
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
RUN useradd --system --no-create-home backend

COPY --from=builder /app/backend/target/release/backend /usr/local/bin/backend
EXPOSE 8080
ENV RUST_LOG=info
USER backend
CMD ["backend"]
