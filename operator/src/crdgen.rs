use kube::CustomResourceExt;
use mycelium::{MinecraftProxy, MinecraftSet};

fn main() {
    println!("{}", serde_yaml::to_string(&MinecraftSet::crd()).unwrap());
    println!("{}", serde_yaml::to_string(&MinecraftProxy::crd()).unwrap());
}
