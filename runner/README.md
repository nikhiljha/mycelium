# mycelium-runner

A simple rust program that configures and wraps a Minecraft server or proxy for use with the [mycelium operator](https://github.com/nikhiljha/mycelium).

## What does it do?

1. copies everything from `MYCELIUM_CONFIG_PATH` to `MYCELIUM_DATA_PATH`
2. edits or creates a new `velocity.toml` or `paper.yml` that contains the `MYCELIUM_FW_TOKEN`
3. downloads plugins from the URLs given in `MYCELIUM_PLUGINS`, puts them in `plugins/`
4. downloads and starts `velocity.jar` or `paper.jar` depending on `MYCELIUM_RUNNER_KIND`
