use std::collections::HashMap;
use std::fmt::Debug;
use std::iter::Map;
use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::StreamExt;
use k8s_openapi::api::apps::v1::StatefulSet;
use k8s_openapi::api::core::v1::{ConfigMapVolumeSource, Volume, VolumeMount};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::{Api, Client, Resource};
use kube::api::ListParams;
use kube_runtime::Controller;
use kube_runtime::controller::{Context, ReconcilerAction};
use prometheus::{
    default_registry, HistogramOpts, HistogramVec, IntCounter,
    proto::MetricFamily, register_histogram_vec, register_int_counter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, event, field, info, instrument, Level, Span, trace, warn};

use crate::Error;
use crate::helpers::metrics::Metrics;
use crate::helpers::state::State;
use crate::MinecraftProxy;
use crate::MinecraftSet;
use crate::objects::minecraft_set::MinecraftSetSpec;

pub mod minecraft_set;
pub mod minecraft_proxy;

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone, JsonSchema)]
pub struct ConfigOptions {
    pub name: String,
    pub path: String,
}

pub fn make_volume_mount(co: &ConfigOptions) -> VolumeMount {
    return VolumeMount {
        name: co.name.clone(),
        mount_path: String::from(Path::new("/config/").join(&co.path).to_str().expect("mount path")),
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

pub fn object_to_owner_reference<K: Resource<DynamicType=()>>(
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