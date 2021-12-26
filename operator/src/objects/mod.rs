use std::{
    collections::HashMap,
    fmt::Debug,
    iter::Map,
    ops::Range,
    path::Path,
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::{DateTime, Utc};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use k8s_openapi::{
    api::{
        apps::v1::StatefulSet,
        core::v1::{ConfigMapVolumeSource, ResourceRequirements, Volume, VolumeMount},
    },
    apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference},
};
use k8s_openapi::api::core::v1::{PersistentVolumeClaim, PodSecurityContext, SecurityContext};
use kube::{api::ListParams, Api, Client, Resource};
use kube_runtime::{
    controller::{Context, ReconcilerAction},
    Controller,
};
use prometheus::{
    default_registry, proto::MetricFamily, register_histogram_vec, register_int_counter,
    HistogramOpts, HistogramVec, IntCounter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, event, field, info, instrument, trace, warn, Level, Span};

use crate::{
    helpers::{metrics::Metrics, state::State},
    objects::minecraft_set::MinecraftSetSpec,
    Error, MinecraftProxy, MinecraftSet,
};

pub mod minecraft_proxy;
pub mod minecraft_set;

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct ConfigOptions {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct ContainerOptions {
    pub resources: Option<ResourceRequirements>,
    pub volume: Option<Volume>,
    pub security_context: Option<PodSecurityContext>,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct RunnerOptions {
    pub jar: VersionDouble,
    pub jvm: Option<String>,
    pub config: Option<Vec<ConfigOptions>>,
    pub plugins: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct VersionDouble {
    pub version: String,
    pub build: String,
}

pub fn make_volume_mount(co: &ConfigOptions) -> VolumeMount {
    return VolumeMount {
        name: co.name.clone(),
        mount_path: String::from(
            Path::new("/config/")
                .join(&co.path)
                .to_str()
                .expect("mount path"),
        ),
        ..VolumeMount::default()
    };
}

pub fn make_volume(co: &ConfigOptions) -> Volume {
    return Volume {
        name: co.name.clone(),
        config_map: Some(ConfigMapVolumeSource {
            name: Some(co.name.clone()),
            ..ConfigMapVolumeSource::default()
        }),
        ..Volume::default()
    };
}

pub fn object_to_owner_reference<K: Resource<DynamicType = ()>>(
    meta: ObjectMeta,
) -> Result<OwnerReference, Error> {
    Ok(OwnerReference {
        api_version: K::api_version(&()).to_string(),
        kind: K::kind(&()).to_string(),
        name: meta.name.unwrap(),
        uid: meta.uid.unwrap(),
        ..OwnerReference::default()
    })
}
