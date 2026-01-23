use crate::containers;
use crate::crd::NodeSpec;
use crate::types::common::{Common, ipfs_ports};
use k8s_openapi::api::apps::v1::{StatefulSet, StatefulSetSpec};
use k8s_openapi::api::core::v1::{PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use kube::api::{ObjectMeta, Patch, PatchParams};
use kube::{Api, Client, Error};
use operator_common::types::statefulset;
use operator_common::{
    external_address_name,
    types::{configmap, load_balancer},
};
use std::collections::BTreeMap;
use tracing::{Level, event, instrument};

#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: String,
    namespace: String,
    spec: NodeSpec,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<StatefulSet, Error> {
    let mut common = Common::new(
        client.clone(),
        name.clone(),
        None,
        namespace.clone(),
        spec.clone(),
        labels.clone(),
    )
    .await?;
    common.create_lb(client.clone(), ipfs_ports()).await?;
    common.generate_configs(client.clone()).await?;

    let object: StatefulSet = StatefulSet {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.0.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(StatefulSetSpec {
            replicas: Some(spec.replicas),
            service_name: "ipfsnode".to_owned(),
            selector: LabelSelector {
                match_expressions: None,
                match_labels: Some(labels.1.clone()),
            },
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers: vec![containers::ipfs::container(&name, &spec)],
                    volumes: Some(containers::ipfs::volumes(&name, &name)),
                    ..PodSpec::default()
                }),
                metadata: Some(ObjectMeta {
                    labels: Some(labels.0.clone()),
                    ..ObjectMeta::default()
                }),
            },

            volume_claim_templates: Some(vec![containers::pvc(
                "ipfs-data",
                spec.ipfs.persistence,
                labels.clone(),
            )]),
            ..StatefulSetSpec::default()
        }),
        ..StatefulSet::default()
    };

    event!(Level::INFO, name, namespace, "Creating StatefulSet");

    let statefulset_api: Api<StatefulSet> = Api::namespaced(client, namespace.as_str());
    let params = PatchParams::apply(&name);
    statefulset_api
        .patch(&name, &params, &Patch::Apply(&object))
        .await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting StatefulSet");
    statefulset::delete(client.clone(), name.clone(), namespace.clone()).await?;
    configmap::delete(
        client.clone(),
        external_address_name(&name),
        namespace.clone(),
    )
    .await?;

    configmap::delete(client.clone(), format!("{name}-configs"), namespace.clone()).await?;
    configmap::delete(
        client.clone(),
        format!("{name}-identities"),
        namespace.clone(),
    )
    .await?;

    configmap::delete(
        client.clone(),
        format!("{name}-startup-scripts"),
        namespace.clone(),
    )
    .await?;

    configmap::delete(
        client.clone(),
        format!("{name}-identities"),
        namespace.clone(),
    )
    .await?;

    load_balancer::delete(client, name, namespace).await
}
