FROM rust:bookworm as builder

WORKDIR /usr/src/php-rs
COPY . .

# Build the release binary
RUN cargo build --release --bin php-fpm

# Runtime image
FROM debian:bookworm-slim

# Install necessary runtime dependencies (e.g., openssl if linked dynamically)
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /usr/src/php-rs/target/release/php-fpm /usr/local/bin/php-fpm

# Expose FPM port
EXPOSE 9000

# Command to run FPM (listening on 0.0.0.0:9000)
CMD ["php-fpm", "--bind", "0.0.0.0:9000"]
