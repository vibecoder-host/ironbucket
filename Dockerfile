FROM rust:1.82 as builder

WORKDIR /app

# Copy dependencies
COPY Cargo.toml ./

# Create dummy main to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    mkdir -p src/bin && echo "fn main() {}" > src/bin/replicator.rs
RUN cargo build --release --bin ironbucket

# Copy actual source code
COPY src ./src

# Force rebuild of the application
RUN touch src/main.rs
RUN cargo build --release --bin ironbucket

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/ironbucket /app/ironbucket

EXPOSE 9000

CMD ["/app/ironbucket"]