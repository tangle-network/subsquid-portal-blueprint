use crate::{PortalInstanceInfo, SubsquidPortalContext};
use blueprint_sdk::error::Error as BlueprintError;
use blueprint_sdk::extract::Context;
use blueprint_sdk::tangle::extract::{List, Optional, TangleArgs5, TangleResult};
use dockworker::config::EnvironmentVars;
use dockworker::config::volume::Volume;
use dockworker::parser::ComposeParser;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

pub const PROVISION_PORTAL_JOB_ID: u8 = 1;

pub async fn provision_portal(
    Context(ctx): Context<SubsquidPortalContext>,
    TangleArgs5(
        network,
        key_path_on_host,
        rpc_url,
        Optional(l1_rpc_url),
        Optional(boot_nodes)
    ): TangleArgs5<
        String,
        String,
        String,
        Optional<String>,
        Optional<List<String>>,
    >,
) -> Result<TangleResult<String>, BlueprintError> {
    let compose_file_path = ctx.base_data_dir.join("subsquid-docker-compose.yml");

    if !compose_file_path.exists() {
        return Err(BlueprintError::Other(
            format!("Compose file not found at: {}", compose_file_path.display()).into(),
        ));
    }

    let mut config = match ComposeParser::new().parse_from_path(&compose_file_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            return Err(BlueprintError::Other(
                format!("Failed to parse compose file: {}", e).into(),
            ));
        }
    };

    let service_name = "query_gateway";
    if let Some(service) = config.services.get_mut(service_name) {
        let mut env_map = HashMap::new();
        env_map.insert("NETWORK".to_string(), network);
        env_map.insert("RPC_URL".to_string(), rpc_url);
        if let Some(l1_url) = l1_rpc_url {
            env_map.insert("L1_RPC_URL".to_string(), l1_url);
        }
        if let Some(nodes) = boot_nodes {
            env_map.insert("BOOT_NODES".to_string(), nodes.0.join(" "));
        }
        let key_path_in_container = "/app/key/network.key";
        env_map.insert("KEY_PATH".to_string(), key_path_in_container.to_string());

        let key_volume = Volume::Bind {
            source: key_path_on_host,
            target: key_path_in_container.to_string(),
            read_only: true,
        };
        service
            .volumes
            .get_or_insert_with(Vec::new)
            .push(key_volume);

        let host_port = 8088u16;
        let port_mapping = format!("{}:8000", host_port);
        service
            .ports
            .get_or_insert_with(Vec::new)
            .push(port_mapping);

        service.environment = Some(EnvironmentVars::from(env_map));

        // --- Extract internal container network info (Best effort) ---
        // We need the container's IP and internal port (8000) for the gateway later.
        // Dockworker's deploy doesn't directly return IP easily.
        // We'll store the host port and container ID; gateway setup will inspect.
    } else {
        return Err(BlueprintError::Other(
            format!("Service '{}' not found in compose file", service_name).into(),
        ));
    }

    let container_ids = match ctx
        .docker_builder
        .deploy_compose_with_base_dir(&mut config, ctx.base_data_dir.clone())
        .await
    {
        Ok(ids) => ids,
        Err(e) => {
            return Err(BlueprintError::Other(
                format!("Failed to deploy compose configuration: {}", e).into(),
            ));
        }
    };

    if let Some(container_id) = container_ids.get(service_name) {
        ctx.instance_counter.fetch_add(1, Ordering::SeqCst);

        // --- Store Instance Info ---
        // We need to inspect the container to get its *internal* IP address reliably.
        // This might fail if the container isn't ready immediately.
        let container_info = ctx
            .docker_builder
            .inspect_container(container_id, None)
            .await
            .map_err(|e| BlueprintError::Other(e.to_string().into()))?;
        // Find the internal IP within the compose network bridge
        let internal_ip = container_info
            .network_settings
            .as_ref()
            .and_then(|net_settings| {
                net_settings.networks.as_ref().and_then(|networks| {
                    networks
                        .values()
                        .find_map(|network| network.ip_address.as_ref().cloned())
                })
            });

        let internal_addr = match internal_ip {
            Some(ip) if !ip.is_empty() => format!("{}:8000", ip), // 8000 is the internal port from compose
            _ => {
                // Fallback or error - cannot reliably determine internal IP
                return Err(BlueprintError::Other(
                    format!(
                        "Failed to determine internal IP address for container {}",
                        container_id
                    )
                    .into(),
                ));
            }
        };

        // Extract host port reliably from inspect result
        let host_port = container_info
            .host_config
            .as_ref()
            .and_then(|hc| hc.port_bindings.as_ref())
            .and_then(|pb| pb.get("8000/tcp"))
            .and_then(|maybe_bindings| maybe_bindings.as_ref())
            .and_then(|bindings| bindings.first())
            .and_then(|binding| binding.host_port.as_ref())
            .and_then(|p_str| p_str.parse::<u16>().ok())
            .unwrap_or(8088);

        let instance_info = PortalInstanceInfo {
            container_id: container_id.clone(),
            internal_addr,
            host_port,
            gateway_config: None, // No gateway configured initially
        };
        ctx.portal_instances
            .insert(container_id.clone(), instance_info);
        // --- End Store Instance Info ---

        Ok(TangleResult(format!(
            "Successfully provisioned portal. Container ID: {}",
            container_id
        )))
    } else {
        Err(BlueprintError::Other(
            format!(
                "Container ID for service '{}' not found after deployment.",
                service_name
            )
            .into(),
        ))
    }
}
