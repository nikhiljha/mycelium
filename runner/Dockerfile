FROM rust:1.53 as builder

WORKDIR ./mycelium-runner
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
COPY ./src ./src
RUN cargo build --release


FROM openjdk:16.0.1-slim-buster

RUN apt-get update && apt-get install -y curl
RUN apt-get clean autoclean && apt-get autoremove --yes && rm -rf /var/lib/{apt,dpkg,cache,log}/

ENV MYCELIUM_CONFIG_PATH=/config
ENV MYCELIUM_DATA_PATH=/data
RUN mkdir -p /config && mkdir -p /data

COPY --from=builder /mycelium-runner/target/release/mycelium-runner /mycelium-runner
CMD /mycelium-runner
