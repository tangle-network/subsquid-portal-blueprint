// subsquid-portal-blueprint-lib/src/jobs/api_key.rs
use crate::{ApiKeyInfo, SubsquidPortalContext, SubsquidPortalError};
use blueprint_sdk::{
    extract::Context,
    tangle::extract::{Optional, TangleArgs3, TangleResult}, // Args: portal_id, rate_limit, public_key
};
use governor::{Quota, RateLimiter};
use rand::distributions::{Alphanumeric, DistString};
use std::num::NonZeroU32;
use std::sync::Arc;
use tracing::info;

pub const GENERATE_API_KEY_JOB_ID: u8 = 3;

/// Generate a secure random alphanumeric key.
fn generate_secure_key(length: usize) -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), length)
}

/// Job to generate a new API key for a specific portal gateway.
/// The gateway must already be configured via `configure_gateway`.
///
/// Arguments:
/// - `portal_container_id`: ID of the portal container instance.
/// - `limit_per_sec`: Optional rate limit (requests per second) for the new key.
/// - `public_key`: Requester's RSA public key (DER format) for encrypting the returned key.
///
/// Returns:
/// - `TangleResult<String>`: Result containing the base64 encoded, RSA encrypted new API key.
pub async fn generate_api_key(
    Context(ctx): Context<SubsquidPortalContext>,
    TangleArgs3(
        portal_container_id,
        Optional(limit_per_sec),
        public_key, // Expect Vec<u8> for public key bytes
    ): TangleArgs3<String, Optional<u32>, Vec<u8>>,
) -> Result<TangleResult<String>, SubsquidPortalError> {
    // Return BlueprintError

    // --- 1. Find Portal Instance & Get Gateway Config ---
    let mut instance = ctx
        .portal_instances
        .get_mut(&portal_container_id)
        .ok_or_else(|| {
            // Use custom error from lib.rs
            SubsquidPortalError::InstanceNotFound(portal_container_id.clone())
        })?;

    let gateway_config = instance
        .gateway_config
        .as_mut()
        .ok_or_else(|| SubsquidPortalError::GatewayNotConfigured(portal_container_id.clone()))?;

    // --- 2. Generate Key and Rate Limiter ---
    let new_key = generate_secure_key(32); // Generate a 32-character alphanumeric key

    let rate_limiter = limit_per_sec
        .and_then(NonZeroU32::new)
        // Add Clock generic
        .map(|limit| Arc::new(RateLimiter::keyed(Quota::per_second(limit))));

    let api_key_info = ApiKeyInfo { rate_limiter };

    // --- 3. Add Key to Config ---
    gateway_config
        .api_keys
        .insert(new_key.clone(), api_key_info);

    info!("Generated new API key for portal {}", portal_container_id);

    // --- 4. Encrypt the key with the requester's public key ---
    let encrypted_key = ctx
        .encrypt_with_public_key(new_key.as_bytes(), &public_key)
        .await?;

    // Encode encrypted bytes as base64 string for TangleResult
    let encrypted_key_str = base64::encode(&encrypted_key);

    Ok(TangleResult(encrypted_key_str))
}
