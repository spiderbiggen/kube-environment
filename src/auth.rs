use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use serde::Deserialize;
use tracing::{error, instrument};

use crate::models::AppState;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct User {
    pub allowed_apps: Vec<String>,
    pub allowed_images: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AuthState(pub User);

#[async_trait]
impl FromRequestParts<AppState> for AuthState {
    type Rejection = StatusCode;

    #[instrument(name = "AuthState", ret, skip_all, fields(
        method = % parts.method, uri = % parts.uri, config = ? state.config
    ))]
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session_token = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let client = state.reqwest_client.clone();
        let req = client
            .get(state.config.openid_uri.clone())
            .header("Authorization", session_token);
        let response = req.send().await;

        match response {
            Ok(response) if response.status().is_success() => {
                match response.json::<User>().await {
                    Ok(user) => Ok(AuthState(user)),
                    Err(error) => {
                        error!(error =% error, "Failed to parse user info");
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }
            Ok(response) => match response.status() {
                StatusCode::UNAUTHORIZED => Err(StatusCode::UNAUTHORIZED),
                StatusCode::FORBIDDEN => Err(StatusCode::FORBIDDEN),
                status => {
                    let body: Option<serde_json::Value> = response.json().await.ok();
                    error!(status =% status, body =? body, "Failed to parse user info");
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            },
            Err(e) => {
                error!(error =% e, "Failed to get user info");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}
