# mycelium

> 🌎 Deploy planet-scale Minecraft server networks on Kubernetes

Mycelium is a Kubernetes controller that enables you to orchestrate and bridge together a large number of Minecraft servers--all with minimal required configuration.

## Installation

> :warning: By default, any software with access to your internal cluster network has full control over your Minecraft servers. Work to stop this [is ongoing](https://github.com/nikhiljha/mycelium/issues/1), so you should not use mycelium unless you understand the consequences of this.

```bash
helm repo add mycelium https://harbor.ocf.berkeley.edu/chartrepo/mycelium
kubectl create ns mycelium
helm install mycelium/mycelium -n mycelium
```

## Usage

Create MinecraftProxy CRDs representing proxies, and MinecraftSet CRDs representing servers. Below is a minimal example, but the full spec is available in the docs.

> :warning: The `mycelium.njha.dev/v1beta1` apiVersion is unstable and may change from release to release, even across minor versions. It will, however, not change across patch versions.

<table align="center">
<tr>
<th>MinecraftProxy</th>
<th>MinecraftSet</th>
</tr>
<tr>
<td>

```yaml
kind: MinecraftProxy
apiVersion: mycelium.njha.dev/v1beta1
metadata:
  name: proxy
spec:
  replicas: 1
  selector:
    matchLabels:
      mycelium.njha.dev/proxy: cluster
  runner:
    jar:
      type: velocity
      version: 3.1.2-SNAPSHOT
      build: "110"
    jvm: "-Xmx4G -Xms4G"
```

</td>
<td>

```yaml
kind: MinecraftSet
apiVersion: mycelium.njha.dev/v1beta1
metadata:
  name: testing
  labels:
    mycelium.njha.dev/proxy: cluster
spec:
  replicas: 3
  runner:
    jar:
      type: paper
      version: 1.18.1
      build: "114"
    jvm: "-Xmx2G -Xms2G"
  container:
    volumeClaimTemplate:
      metadata:
        name: root
      spec:
        accessModes: ["ReadWriteOnce"]
        storageClassName: openebs-zfspv
        resources:
          requests:
            storage: 64Gi
```

</td>
</tr>
</table>

## Internals

### Components

- `mycelium-operator` - A Kubernetes operator that listens for changes to `MinecraftSet` and `MinecraftProxy` CRDs and links them together by creating other Kubernetes objects (like `Service`, `StatefulSet`).
- `mycelium-runner` - A Rust binary that acts as the entrypoint to proxy or game containers. It downloads server jars, plugins, and automatically edits configuration files to work how the operator expects.
- `mycelium-velocity` - A Velocity plugin that 1) provides useful HTTP endpoints for `mycelium-operator` to interact with 2) pings `mycelium-operator` periodically to sync changes 3) collects monitoring information.
- `mycelium-paper` - A PaperMC plugin that 1) exposes useful HTTP endpoints 2) collects monitoring information, exposing it to Kubernetes itself and `mycelium-operator`.

### Goals

Keep in mind this project is in early alpha. This project doesn't meet very many of its goals yet, but, in no particular order...
- cloud native monitoring, tracing, and observability
- fault tolerance with redundant servers, proxies
  - (we're currently looking into dynamically moving players to a new endpoint, resulting in just a minor lag spike if an entire proxy goes down)
- declarative, eventually-consistent server configuration
- speed (minimal convergence time between cluster state and Minecraft state)
- security (encryption backends for secrets, config management, rbac)
