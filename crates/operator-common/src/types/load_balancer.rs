use k8s_openapi::api::core::v1::Service;
use kube::{Api, Client, Error, ResourceExt, api::ListParams, core::ErrorResponse};
use kube_runtime::wait::{Condition, await_condition};
use std::{collections::BTreeMap, time::Duration};
use tokio::task::JoinSet;
use tracing::instrument;

use crate::{
    labels, selector_labels,
    types::service::{self, Port, ServiceType},
};

#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: String,
    namespace: String,
    kind: String,
    replicas: i32,
    ports: Vec<Port>,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<(), crate::Error> {
    let service_api: Api<Service> = Api::namespaced(client.clone(), namespace.as_str());
    let lp = ListParams::default()
        .match_any()
        .timeout(300)
        .labels(format!("app.kubernetes.io/instance={name}").as_str())
        .labels("app.kubernetes.io/name=ipfs-storage-cluster")
        .fields("spec.type=LoadBalancer");
    let existing_load_balancers = service_api.list(&lp).await?;
    let lb_count = existing_load_balancers.items.len();

    if lb_count > replicas as usize {
        // Handle excess load balancers
        let mut set = JoinSet::new();
        for idx in (replicas as usize)..lb_count {
            let cli = client.clone();
            let n = name.to_owned();
            let ns = namespace.to_owned();

            set.spawn(service::delete(cli, format!("{n}-{idx}"), ns.clone()));
        }

        while let Some(res) = set.join_next().await {
            res??;
        }
    } else if lb_count < replicas as usize {
        // Handle insufficient load balancers
        _create(
            client,
            name,
            namespace,
            kind,
            ports,
            lb_count,
            replicas as usize,
        )
        .await?;
    }

    Ok(())
}

#[instrument(skip(client))]
pub async fn get_external_ips(
    client: Client,
    name: String,
    namespace: String,
    port: Port,
    replicas: i32,
) -> Result<BTreeMap<String, String>, crate::Error> {
    let mut external_addrs: BTreeMap<String, String> = BTreeMap::new();

    let mut set = JoinSet::new();
    for idx in 0..replicas {
        let cli = client.clone();
        let n = name.to_owned();
        let ns = namespace.to_owned();

        set.spawn(async move {
            wait(cli, format!("{n}-{idx}"), ns)
                .await
                .map(|ip_address| (format!("{n}-{idx}"), ip_address))
        });
    }

    while let Some(res) = set.join_next().await {
        let (pod_name, ip_address) = res??;
        external_addrs.insert(pod_name, ip_address);
    }

    Ok(external_addrs)
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    let service_api: Api<Service> = Api::namespaced(client.clone(), namespace.as_str());
    let lp = ListParams::default()
        .match_any()
        .timeout(300)
        .labels(format!("app.kubernetes.io/instance={name}").as_str())
        .fields("spec.type=LoadBalancer");
    let existing_load_balancers = service_api.list(&lp).await?;

    let mut set = JoinSet::new();
    for lb in existing_load_balancers {
        let cli = client.clone();
        let ns = namespace.to_owned();

        set.spawn(service::delete(cli, lb.name_any(), ns.clone()));
    }

    while let Some(res) = set.join_next().await {
        match res {
            Ok(_) => (),
            Err(err) => {
                return Err(Error::Api(ErrorResponse {
                    status: "Failed".to_string(),
                    message: err.to_string(),
                    reason: "Failed to join set while deleting load balancers".to_string(),
                    code: 418,
                }));
            }
        }
    }

    Ok(())
}

#[instrument(skip(client))]
pub async fn wait(
    client: Client,
    name: String,
    namespace: String,
) -> std::result::Result<String, crate::Error> {
    let service_api: Api<Service> = Api::namespaced(client, namespace.as_str());

    let exists = await_condition(service_api, name.as_str(), external_ip_exists());
    let out = tokio::time::timeout(Duration::from_secs(300), exists).await?;
    match out {
        Ok(svc) => {
            let status = svc
                .and_then(|s| s.status)
                .and_then(|st| st.load_balancer)
                .ok_or(crate::Error::IngressListMissing)?;
            match status.ingress {
                Some(ingress) => {
                    if let Some(first) = ingress.first() {
                        if let Some(ip) = &first.ip {
                            Ok(ip.clone())
                        } else {
                            Err(crate::Error::IPTimeout)
                        }
                    } else {
                        Err(crate::Error::IngressListEmpty)
                    }
                }
                None => Err(crate::Error::IngressListMissing),
            }
        }
        Err(e) => Err(crate::Error::WaitError { source: e }),
    }
}

#[instrument]
fn external_ip_exists() -> impl Condition<Service> {
    move |obj: Option<&Service>| {
        if let Some(svc) = &obj
            && let Some(status) = &svc.status
            && let Some(lb) = &status.load_balancer
            && let Some(ingress) = &lb.ingress
            && let Some(first) = ingress.first()
            && first.ip.is_some()
        {
            return true;
        }
        false
    }
}

async fn _create(
    client: Client,
    name: String,
    namespace: String,
    kind: String,
    ports: Vec<Port>,
    lower: usize,
    upper: usize,
) -> Result<(), crate::Error> {
    let mut set = JoinSet::new();

    for idx in lower..upper {
        let pod_name = format!("{name}-{idx}");
        let mut sl = selector_labels(name.clone(), kind.clone().to_string());
        sl.insert(
            "statefulset.kubernetes.io/pod-name".to_owned(),
            pod_name.clone(),
        );

        let cli = client.clone();
        let n = name.to_owned();
        let ns = namespace.to_owned();

        set.spawn(service::deploy(
            cli,
            format!("{n}-{idx}"),
            ns,
            ServiceType::LoadBalancer,
            ports.clone(),
            (labels(name.clone(), kind.clone().to_string()), sl),
        ));
    }

    while let Some(res) = set.join_next().await {
        res??;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{LoadBalancerIngress, LoadBalancerStatus, ServiceStatus};

    #[test]
    fn test_external_ip_exists_condition() {
        let condition = external_ip_exists();

        // None service
        assert!(!condition.matches_object(None));

        // Service with None status
        let svc_no_status = Service::default();
        assert!(!condition.matches_object(Some(&svc_no_status)));

        // Service with status but None load_balancer
        let svc_no_lb = Service {
            status: Some(ServiceStatus::default()),
            ..Default::default()
        };
        assert!(!condition.matches_object(Some(&svc_no_lb)));

        // Service with empty ingress list (Must NOT panic!)
        let svc_empty_ingress = Service {
            status: Some(ServiceStatus {
                load_balancer: Some(LoadBalancerStatus {
                    ingress: Some(vec![]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!condition.matches_object(Some(&svc_empty_ingress)));

        // Service with ingress having None ip
        let svc_no_ip = Service {
            status: Some(ServiceStatus {
                load_balancer: Some(LoadBalancerStatus {
                    ingress: Some(vec![LoadBalancerIngress {
                        hostname: Some("example.com".into()),
                        ip: None,
                        ..Default::default()
                    }]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!condition.matches_object(Some(&svc_no_ip)));

        // Service with valid IP
        let svc_with_ip = Service {
            status: Some(ServiceStatus {
                load_balancer: Some(LoadBalancerStatus {
                    ingress: Some(vec![LoadBalancerIngress {
                        ip: Some("192.168.1.100".into()),
                        ..Default::default()
                    }]),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(condition.matches_object(Some(&svc_with_ip)));
    }
}
