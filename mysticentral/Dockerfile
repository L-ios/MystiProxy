# MystiCentral Dockerfile
# Multi-stage build for optimized image size

# Stage 1: Build
FROM rust:1.75 AS builder

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY mysticentral ./mysticentral
COPY http_proxy ./http_proxy
COPY mystictl ./mystictl
COPY src ./src

# Build the mysticentral binary
RUN cargo build --release -p mysticentral

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false appuser

WORKDIR /app

# Copy the binary
COPY --from=builder /app/target/release/mysticentral /usr/local/bin/mysticentral

# Set ownership
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose the application port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run the binary
CMD ["mysticentral"]
