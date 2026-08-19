FROM rust:1-bookworm AS builder

RUN rustup target add wasm32-unknown-unknown \
    && curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

WORKDIR /src
COPY . .

WORKDIR /src/frontend
RUN wasm-pack build --no-pack --target web

WORKDIR /src
RUN cargo build --release -p backend

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/backend /app/backend/backend
COPY --from=builder /src/frontend/pkg /app/frontend/pkg
COPY --from=builder /src/frontend/public /app/frontend/public
COPY backend/keys.txt /app/backend/keys.txt

WORKDIR /app/backend
EXPOSE 8000
CMD ["./backend"]
