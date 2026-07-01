# syntax=docker/dockerfile:1

# Build stage: compile sunrise-server with the official Rust image.
FROM rust:1-trixie AS builder
WORKDIR /src

# Copy workspace manifests first to cache dependency builds.
COPY Cargo.toml Cargo.lock ./
COPY sunrise-server/Cargo.toml ./sunrise-server/Cargo.toml
COPY firmware/Cargo.toml ./firmware/Cargo.toml

# Pre-build dependencies with a stub main so the dep layer caches.
RUN mkdir -p sunrise-server/src \
    && echo "fn main() {}" > sunrise-server/src/main.rs \
    && cargo build --locked --release -p sunrise-server \
    && rm -rf sunrise-server/src

# Copy the real source and build the binary.
COPY sunrise-server/src ./sunrise-server/src
RUN touch sunrise-server/src/main.rs \
    && cargo build --locked --release -p sunrise-server

# Runtime stage: distroless glibc image.
FROM gcr.io/distroless/cc-debian13
COPY --from=builder /src/target/release/sunrise-server /usr/local/bin/sunrise-server
EXPOSE 41399
ENTRYPOINT ["/usr/local/bin/sunrise-server"]
