# mycelium

Deploy planet-scale Minecraft server networks with state-of-the-art fault tolerance, observability, and monitoring.

`mycelium` is a tightly-integrated set of tools to help you build Minecraft networks on Kubernetes. It's composed of a few parts, all contained within this monorepo.

- `mycelium-operator` - A Kubernetes operator that listens for changes to `MinecraftSet` and `MinecraftProxy` CRDs and links them together by creating other Kubernetes objects (like `Service`, `StatefulSet`).
- `mycelium-runner` - A Rust binary that acts as the entrypoint to proxy or gameserver containers. It downloads server jars, plugins, and automatically edits configuration files to work how the operator expects.
- `mycelium-velocity` - A Velocity plugin that 1) provides useful HTTP endpoints for `mycelium-operator` to interact with 2) pings `mycelium-operator` periodically to see if it missed any changes 3) (WIP) hooks into performance and error monitoring plugins to send that data elsewhere in the cluster (e.x. elasticsearch, tremor, prometheus).
- `mycelium-paper` - A PaperMC plugin that collects monitoring and information, exposing it to Kubernetes itself and `mycelium-operator`.

## Project Goals

Keep in mind this project is in early alpha. This project doesn't meet very many of it's goals yet, but, in no particular order...
- state-of-the-art cloud native monitoring, tracing, and observability
- fault tolerance with redundant servers, proxies 
  - (we're currently looking into dynamically moving players to a new endpoint, resulting in just a minor lag spike if an entire proxy goes down)
- declarative, eventually-consistent server configuration
- speed (minimal convergence time between cluster state and Minecraft state)
- security (encryption backends for secrets, config management, rbac)

## Setup

This section is WIP! If you're not me, you shouldn't be using this just yet.

### Requirements
- A Kubernetes Cluster w/ working `PersistentVolumeClaim`s
- OpenTelemetry Collector (optional, for production)

### CRD
Generate the CRD from the rust types and apply it to your cluster:

```sh
cargo run --bin mycelium-crdgen | kubectl apply -f -
```

### Telemetry
When using the `telemetry` feature, you need an opentelemetry collector configured. Anything should work, but you might need to change the exporter in `main.rs` if it's not grpc otel.

Otherwise, run without the `telemetry` feature via: `cargo run`.

