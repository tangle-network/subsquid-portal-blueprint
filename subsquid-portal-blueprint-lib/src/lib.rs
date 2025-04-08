use blueprint_sdk::error::Error as BlueprintError;
use blueprint_sdk::macros::context::{KeystoreContext, ServicesContext, TangleClientContext};
use blueprint_sdk::runner::config::BlueprintEnvironment;
use dashmap::DashMap;
use dockworker::{DockerBuilder, error::DockerError};
use governor::RateLimiter;
use governor::clock::DefaultClock;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use thiserror::Error;
use tokio::task::JoinHandle;

// --- Modules ---
mod gateway;
mod jobs;

// --- Re-export jobs ---
pub use jobs::api_key::{GENERATE_API_KEY_JOB_ID, generate_api_key};
pub use jobs::configure_gateway::{CONFIGURE_GATEWAY_JOB_ID, configure_gateway};
pub use jobs::provision::{PROVISION_PORTAL_JOB_ID, provision_portal};

// --- Data Structures for Context State ---

#[derive(Clone)]
pub struct ApiKeyInfo {
    pub rate_limiter: Option<
        Arc<
            RateLimiter<
                String,
                governor::state::keyed::DefaultKeyedStateStore<String>,
                DefaultClock,
            >,
        >,
    >,
}

#[derive(Clone)]
pub struct GatewayConfig {
    pub listen_port: u16,
    pub api_keys: DashMap<String, ApiKeyInfo>,
}

#[derive(Clone)]
pub struct PortalInstanceInfo {
    pub container_id: String,
    pub internal_addr: String,
    pub host_port: u16,
    pub gateway_config: Option<GatewayConfig>,
}

// --- Context Definition ---
#[derive(Clone, TangleClientContext, ServicesContext, KeystoreContext)]
pub struct SubsquidPortalContext {
    #[config]
    pub env: BlueprintEnvironment,
    pub docker_builder: Arc<DockerBuilder>,
    pub instance_counter: Arc<AtomicUsize>,
    pub base_data_dir: PathBuf,
    pub portal_instances: Arc<DashMap<String, PortalInstanceInfo>>,
    pub gateway_tasks: Arc<DashMap<String, JoinHandle<()>>>,
}

impl SubsquidPortalContext {
    pub async fn new(env: BlueprintEnvironment) -> Result<Self, SubsquidPortalError> {
        let docker_builder = DockerBuilder::new()
            .await
            .map_err(Into::<DockerError>::into)?;
        let base_data_dir = env
            .data_dir
            .clone()
            .ok_or_else(|| {
                SubsquidPortalError::ConfigError(
                    "Data directory not set in environment".to_string(),
                )
            })?
            .join("subsquid_portal");
        tokio::fs::create_dir_all(&base_data_dir)
            .await
            .map_err(|e| {
                SubsquidPortalError::ConfigError(format!("Failed to create data directory: {}", e))
            })?;

        Ok(Self {
            docker_builder: Arc::new(docker_builder),
            instance_counter: Arc::new(AtomicUsize::new(0)),
            base_data_dir,
            portal_instances: Arc::new(DashMap::new()),
            gateway_tasks: Arc::new(DashMap::new()),
            env,
        })
    }

    pub async fn get_portal_instance(
        &self,
        portal_id: &str,
    ) -> Result<PortalInstanceInfo, SubsquidPortalError> {
        self.portal_instances
            .get(portal_id)
            .map(|instance| instance.clone())
            .ok_or(SubsquidPortalError::InstanceNotFound(portal_id.to_string()))
    }

    pub async fn encrypt_with_public_key(
        &self,
        data: &[u8],
        public_key: &[u8],
    ) -> Result<Vec<u8>, SubsquidPortalError> {
        let public_key = RsaPublicKey::from_pkcs1_der(public_key.into())?;
        let mut rng = rand::thread_rng();
        let encrypted_data = public_key.encrypt(&mut rng, Pkcs1v15Encrypt, data)?;
        Ok(encrypted_data)
    }
}

#[derive(Debug, Error)]
pub enum SubsquidPortalError {
    #[error("Dockworker error: {0}")]
    DockerError(#[from] DockerError),
    #[error("Blueprint error: {0}")]
    BlueprintError(#[from] BlueprintError),
    #[error("RSA error: {0}")]
    RsaError(#[from] rsa::Error),
    #[error("RSA PKCS1v15 encryption error: {0}")]
    RsaPkcs1v15EncryptError(#[from] rsa::pkcs1::Error),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Instance not found: {0}")]
    InstanceNotFound(String),
    #[error("Gateway task error: {0}")]
    GatewayTaskError(String),
    #[error("Gateway not configured for portal instance: {0}")]
    GatewayNotConfigured(String),
    #[error("API Key generation error: {0}")]
    ApiKeyGenerationError(String),
}
