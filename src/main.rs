use std::net::SocketAddr;

use anyhow::Result;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::{extract::Path, http::StatusCode, routing::get, Json, Router};
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::Api;
use kube::api::{Patch, PatchParams, ValidationDirective};
use kube::error::Error as KubeError;
use serde::Deserialize;
use serde_json::{json, Value};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::decompression::DecompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::instrument;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::{AuthState, User};
use crate::models::AppState;

mod auth;
mod controllers;
mod models;

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::from_env().await?;

    let app = Router::new()
        .route(
            "/deployments/:name",
            get(controllers::query).patch(controllers::deploy),
        )
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new())
                .layer(DecompressionLayer::new()),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
