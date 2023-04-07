FROM clux/muslrust:1.66.0 as builder
WORKDIR ./volume
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./src ./src
RUN cargo build --release --bin mycelium-operator

FROM gcr.io/distroless/static:nonroot
LABEL org.opencontainers.image.source=https://github.com/nikhiljha/mycelium
COPY --from=builder /volume/volume/target/x86_64-unknown-linux-musl/release/mycelium-operator /app/
EXPOSE 8080
CMD ["/app/mycelium-operator"]
