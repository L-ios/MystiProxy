FROM rust:1.85 AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY mysti-common ./mysti-common
COPY mysticentral ./mysticentral
COPY src ./src

RUN cargo build --release -p mysticentral

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false appuser

WORKDIR /app

COPY --from=builder /app/target/release/mysticentral /usr/local/bin/mysticentral

RUN chown -R appuser:appuser /app

USER appuser

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["mysticentral"]
