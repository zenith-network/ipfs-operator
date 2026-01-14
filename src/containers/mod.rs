use std::collections::BTreeMap;

use k8s_openapi::{
    api::core::v1::{PersistentVolumeClaim, PersistentVolumeClaimSpec, VolumeResourceRequirements},
    apimachinery::pkg::api::resource::Quantity,
};
use kube::api::ObjectMeta;

use crate::Persistence;

pub mod ipfs;
pub mod ipfs_cluster;

pub fn pvc(
    name: &str,
    persistence: Persistence,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> PersistentVolumeClaim {
    let mut requests: BTreeMap<String, Quantity> = BTreeMap::new();
    if let Some(vol_size) = persistence.size {
        requests.insert("storage".to_owned(), Quantity(vol_size));
    }

    PersistentVolumeClaim {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            labels: Some(labels.0.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(PersistentVolumeClaimSpec {
            access_modes: Some(vec![
                persistence
                    .access_mode
                    .unwrap_or("ReadWriteOnce".to_string()),
            ]),
            resources: Some(VolumeResourceRequirements {
                requests: Some(requests),
                ..VolumeResourceRequirements::default()
            }),
            ..PersistentVolumeClaimSpec::default()
        }),
        ..PersistentVolumeClaim::default()
    }
}
