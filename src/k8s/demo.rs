use kube::{
    api::{Api, ListParams, ResourceExt},
    client::Client,
    config,
    core::ObjectMeta,
    runtime::{watcher, WatchStreamExt},
    Error,
};

use futures::prelude::*;
use k8s_openapi::api::core::v1::Pod;
use log::*;

pub async fn demo_k8s() -> Result<(), Error> {
    let client = Client::try_default().await?;
    let api = Api::<Pod>::default_namespaced(client);
    let use_watchlist = std::env::var("WATCHLIST")
        .map(|s| s == "1")
        .unwrap_or(false);
    let wc = if use_watchlist {
        // requires WatchList feature gate on 1.27 or later
        watcher::Config::default().streaming_lists()
    } else {
        watcher::Config::default()
    };

    watcher(api, wc)
        .applied_objects()
        .default_backoff()
        .try_for_each(|p| async move {
            info!("saw {}", p.name_any());
            if let Some(unready_reason) = pod_unready(&p) {
                warn!("{}", unready_reason);
            }
            Ok(())
        })
        .await
        .expect("good expect");
    Ok(())
}

fn pod_unready(p: &Pod) -> Option<String> {
    let status = p.status.as_ref().unwrap();
    if let Some(conds) = &status.conditions {
        let failed = conds
            .iter()
            .filter(|c| c.type_ == "Ready" && c.status == "False")
            .map(|c| c.message.clone().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(",");
        if !failed.is_empty() {
            if p.metadata.labels.as_ref().unwrap().contains_key("job-name") {
                return None; // ignore job based pods, they are meant to exit 0
            }
            return Some(format!("Unready pod {}: {}", p.name_any(), failed));
        }
    }
    None
}
