FROM rust:1.92 AS builder

WORKDIR ./mycelium-runner
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./src ./src
COPY ./defaults ./defaults
RUN cargo build --release --bin mycelium-runner


FROM eclipse-temurin:21-jdk-alpine-3.23
LABEL org.opencontainers.image.source=https://github.com/nikhiljha/mycelium

RUN apk add --no-cache curl

ENV MYCELIUM_CONFIG_PATH=/config
ENV MYCELIUM_DATA_PATH=/data
RUN mkdir -p /config && mkdir -p /data

COPY --from=builder /mycelium-runner/target/release/mycelium-runner /mycelium-runner
CMD ["/mycelium-runner"]
