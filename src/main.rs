#![deny(clippy::all)]
#![warn(clippy::pedantic)]

use std::net::{IpAddr, Ipv6Addr, SocketAddr};

use anyhow::Result;
use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::decompression::DecompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::models::AppState;

mod auth;
mod controllers;
mod models;

const SOCKET: &SocketAddr = &SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8000);

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::from_env().await?;

    let router = Router::new()
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

    let listener = TcpListener::bind(SOCKET).await?;
    info!("listening for requests on {SOCKET}");
    axum::serve(listener, router).await?;
    Ok(())
}
