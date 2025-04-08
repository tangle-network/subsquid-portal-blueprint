// subsquid-portal-blueprint-lib/src/jobs/configure_gateway.rs
use crate::{GatewayConfig, SubsquidPortalContext};
use blueprint_sdk::{
    error::Error as BlueprintError,
    extract::Context,
    tangle::extract::{TangleArgs2, TangleResult}, // Args: portal_id, listen_port
};
use dashmap::DashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tracing::{info, warn};

pub const CONFIGURE_GATEWAY_JOB_ID: u8 = 2;

/// Job to configure and potentially restart the gateway for a specific portal instance.
/// This sets up the main listener and proxy; API keys are managed separately.
///
/// Arguments:
/// - `portal_container_id`: ID of the portal container to front
/// - `listen_port`: Port the gateway should listen on
///
/// Returns:
/// - `TangleResult<String>`: Result containing a success message
pub async fn configure_gateway(
    Context(ctx): Context<SubsquidPortalContext>,
    TangleArgs2(portal_container_id, listen_port): TangleArgs2<String, u16>,
) -> Result<TangleResult<String>, BlueprintError> {
    // Return BlueprintError

    // --- 1. Find Portal Instance ---
    let mut instance = ctx
        .portal_instances
        .get_mut(&portal_container_id)
        .ok_or_else(|| {
            BlueprintError::Other(
                format!(
                    "Portal instance with ID '{}' not found.",
                    portal_container_id
                )
                .into(),
            )
        })?;

    // --- 2. Abort Existing Gateway Task (if any) ---
    if let Some((_, existing_task)) = ctx.gateway_tasks.remove(&portal_container_id) {
        warn!(
            "Aborting existing gateway task for portal {}",
            portal_container_id
        );
        existing_task.abort();
    }

    // --- 3. Prepare/Update Gateway Configuration ---
    // Create a new config or update the port of the existing one
    let gateway_config = Arc::new(match instance.gateway_config.as_ref() {
        Some(existing_config) => {
            // Reuse existing API keys map, just update the port
            GatewayConfig {
                listen_port,
                api_keys: existing_config.api_keys.clone(), // Clone the existing DashMap
            }
        }
        None => {
            // Create a new config with an empty API key map
            GatewayConfig {
                listen_port,
                api_keys: DashMap::new(),
            }
        }
    });

    // --- 4. Update Portal Instance Info ---
    instance.gateway_config = Some(gateway_config.as_ref().clone()); // Store the potentially updated config
    let internal_addr = instance.internal_addr.clone();

    // Drop the mutable borrow before spawning the task
    drop(instance);

    // --- 5. Spawn New Gateway Server Task ---
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), listen_port);

    // Clone the Arc for the spawned task
    let gateway_config_clone = gateway_config.clone();
    let gateway_handle = tokio::spawn(async move {
        // Pass the Arc'd config to the server
        crate::gateway::run_gateway_server(listen_addr, internal_addr, gateway_config_clone).await;
    });

    // --- 6. Store New Task Handle ---
    ctx.gateway_tasks
        .insert(portal_container_id.clone(), gateway_handle);

    info!(
        "Gateway configured and started for portal {} on port {}",
        portal_container_id, listen_port
    );

    Ok(TangleResult(format!(
        "Gateway configured for portal {}. Listening on port {}.",
        portal_container_id, listen_port
    )))
}
