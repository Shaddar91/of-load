# syntax=docker/dockerfile:1.7

FROM rust:1.96-alpine AS build

ARG CARGO_BUILD_JOBS=4

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && \
    echo 'fn main() { println!("cache-primer"); }' > src/main.rs && \
    cargo build --release --locked && \
    rm -rf src target/release/of-load*

COPY levels.json ./
COPY src ./src
RUN find src -type f -exec touch {} + && \
    cargo build --release --locked && \
    strip target/release/of-load && \
    test "$(stat -c%s target/release/of-load)" -gt 1000000 || \
        (echo "FATAL: of-load binary is suspiciously small — cache-primer leak" && exit 1)

FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=build /src/target/release/of-load /of-load

EXPOSE 8000
USER 65532:65532
ENTRYPOINT ["/of-load"]
