# Development Dockerfile for rqbit-fuse
# This allows building and testing the Linux version on macOS

FROM rust:1.75-slim-bookworm

# Install required system dependencies for FUSE development
RUN apt-get update && apt-get install -y \
    libfuse-dev \
    pkg-config \
    build-essential \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install cargo-watch for development
RUN cargo install cargo-watch

# Create app directory
WORKDIR /app

# Copy Cargo files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Build dependencies only (this layer will be cached)
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build && rm -rf src

# Default command: run tests
CMD ["cargo", "test"]
