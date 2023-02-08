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
use tracing::instrument;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::{AuthState, User};
use crate::models::AppState;

mod auth;
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
        .route("/deployments/:name", get(query).patch(deploy))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct DeployQuery {
    image: String,
}

#[instrument]
async fn query(
    Path(name): Path<String>,
    AuthState(user): AuthState,
    State(state): State<AppState>,
) -> Result<Json<Value>, Response> {
    validate_allowed_app(&user, &name)?;

    let deployment_api: Api<Deployment> = Api::default_namespaced(state.kube_client);
    let deployment = get_deployment(&deployment_api, &name).await?;
    Ok(Json(deployment_to_json(deployment)))
}

#[instrument]
async fn deploy(
    Path(name): Path<String>,
    Query(query): Query<DeployQuery>,
    AuthState(user): AuthState,
    State(state): State<AppState>,
) -> Result<Json<Value>, Response> {
    validate_allowed_app(&user, &name)?;
    validate_allowed_image(&user, &query.image)?;

    let deployment_api: Api<Deployment> = Api::default_namespaced(state.kube_client);

    match patch_deployment_image(deployment_api, &name, &query.image).await {
        Ok(patched) => Ok(Json(deployment_to_json(patched))),
        Err(e) => {
            tracing::error!(error =%e, "failed to patch deployment");
            Err(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

async fn patch_deployment_image(
    deployment_api: Api<Deployment>,
    name: &str,
    image: &str,
) -> Result<Deployment, KubeError> {
    let params = PatchParams {
        dry_run: false,
        force: false,
        field_manager: Some("kube-environment".into()),
        field_validation: Some(ValidationDirective::Strict),
    };
    let patch = json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "spec": {
            "template": {
                "spec": {
                    "containers": [{
                        "name": name,
                        "image": image,
                        "imagePullPolicy": "IfNotPresent"
                    }]
                }
            }
        }
    });
    let patch = Patch::Apply(patch);
    deployment_api.patch(name, &params, &patch).await
}

async fn get_deployment(api: &Api<Deployment>, name: &str) -> Result<Deployment, Response> {
    api.get(name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn deployment_to_json(mut deployment: Deployment) -> Value {
    deployment.metadata.managed_fields = None;
    json!(deployment)
}

fn validate_allowed_app(user: &User, name: &String) -> Result<(), Response> {
    if user.allowed_apps.contains(&name) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN.into_response())
    }
}

fn validate_allowed_image(user: &User, image: &str) -> Result<(), Response> {
    let option = image.rsplit_once(':');
    if let Some((unversioned_image, _)) = option {
        if user.allowed_images.iter().any(|s| s == unversioned_image) {
            return Ok(());
        }
    }

    let pair = (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "only allowed images can be deployed",
            "allowed_images": user.allowed_images,
        })),
    );
    Err(pair.into_response())
}
