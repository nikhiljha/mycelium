FROM --platform=linux/amd64 clux/muslrust:stable AS builder
WORKDIR /volume
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./src ./src
COPY ./defaults ./defaults
RUN cargo build --release --bin mycelium-operator

FROM gcr.io/distroless/static:nonroot
LABEL org.opencontainers.image.source=https://github.com/nikhiljha/mycelium
COPY --from=builder /volume/target/x86_64-unknown-linux-musl/release/mycelium-operator /app/
EXPOSE 8080
CMD ["/app/mycelium-operator"]
