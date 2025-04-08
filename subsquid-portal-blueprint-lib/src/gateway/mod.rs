use crate::GatewayConfig; // Import config structs
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::any,
};
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info, warn};

// Use the Arc'd GatewayConfig in the AppState
#[derive(Clone)]
struct AppState {
    http_client: Client,
    target_portal_addr: String,
    gateway_config: Arc<GatewayConfig>,
}

// Accept Arc<GatewayConfig>
pub async fn run_gateway_server(
    listen_addr: SocketAddr,
    target_portal_addr: String,
    gateway_config: Arc<GatewayConfig>,
) {
    let state = AppState {
        http_client: Client::new(),
        target_portal_addr: target_portal_addr.clone(),
        gateway_config,
    };

    let app = Router::new()
        .route("/*path", any(proxy_handler))
        .with_state(state);

    info!(
        "Starting gateway server on {} proxying to {}",
        listen_addr, target_portal_addr
    );

    tokio::spawn(async move {
        let listener_result = tokio::net::TcpListener::bind(listen_addr).await;
        match listener_result {
            Ok(listener) => {
                if let Err(e) = axum::serve(listener, app).await {
                    if !e
                        .to_string()
                        .contains("hyper server error: connection closed")
                    {
                        error!("Gateway server error: {}", e);
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to bind gateway server listener {}: {}",
                    listen_addr, e
                );
            }
        }
        info!("Gateway server on {} has shut down.", listen_addr);
    });
}

// Helper function to convert axum Body to reqwest Body
async fn axum_body_to_reqwest(body: Body) -> Result<reqwest::Body, hyper::Error> {
    // Axum's Body implements HttpBody, which has a `data()` method returning Option<Result<Bytes>>
    // We can stream these bytes.
    let stream = body.into_data_stream();
    let reqwest_body = reqwest::Body::wrap_stream(stream);
    Ok(reqwest_body)
}

async fn proxy_handler(
    State(state): State<AppState>,
    req: Request<Body>, // Keep axum Body initially
) -> Result<Response, StatusCode> {
    // --- 1. Authentication ---
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    let api_key_str = match auth_header {
        Some(key) => key,
        None => {
            warn!("Missing or invalid Authorization header");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Find the key (String) and its associated info in our config map
    // We need to borrow the key string for the lookup
    let api_key_info = match state.gateway_config.api_keys.get(api_key_str) {
        Some(info_ref) => info_ref.value().clone(),
        None => {
            warn!("Unauthorized API key provided");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // --- 2. Rate Limiting ---
    if let Some(limiter) = &api_key_info.rate_limiter {
        if let Err(_) = limiter.check_key(&api_key_str.to_string()) {
            warn!(
                "Rate limit exceeded for key starting with: {}",
                &api_key_str[0..std::cmp::min(api_key_str.len(), 4)]
            );
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    // --- 3. Proxying ---
    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(path);
    let target_uri_str = format!("http://{}{}", state.target_portal_addr, path_query);

    let target_url = match target_uri_str.parse::<reqwest::Url>() {
        Ok(url) => url,
        Err(e) => {
            error!("Failed to parse target URL '{}': {}", target_uri_str, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Clone essential parts from the original request
    let method = req.method().clone();
    let mut headers = req.headers().clone();
    let axum_body = req.into_body(); // Take ownership of the body

    headers.remove(header::AUTHORIZATION);

    // Convert body type
    let reqwest_body = match axum_body_to_reqwest(axum_body).await {
        Ok(body) => body,
        Err(e) => {
            error!("Failed to process request body: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Log the target URL before building the request
    info!("Proxying request to: {}", target_url);

    let client_req = state
        .http_client
        .request(method, target_url)
        .headers(headers)
        .body(reqwest_body);

    match client_req.send().await {
        Ok(res) => {
            let mut response_builder = Response::builder().status(res.status());
            let headers = res.headers().clone();
            for (key, value) in headers.iter() {
                response_builder = response_builder.header(key, value);
            }
            let body = match res.bytes().await {
                Ok(bytes) => Body::from(bytes),
                Err(e) => {
                    error!("Failed to read response body: {}", e);
                    return Err(StatusCode::BAD_GATEWAY);
                }
            };
            Ok(response_builder.body(body).unwrap_or_else(|e| {
                error!("Failed to build response: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }))
        }
        Err(e) => {
            error!("Failed to proxy request: {}", e);
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}
