use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Patch, PatchParams, ValidationDirective};
use kube::{Api, Error as KubeError};
use lazy_static::lazy_static;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{error, instrument};

use crate::auth::{AuthState, User};
use crate::models::AppState;

lazy_static! {
    static ref PATCH_PARAMS: PatchParams = PatchParams {
        dry_run: false,
        force: false,
        field_manager: Some(String::from(env!("CARGO_BIN_NAME"))),
        field_validation: Some(ValidationDirective::Strict),
    };
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeployQuery {
    image: String,
    container: Option<String>,
    namespace: Option<String>,
}

#[instrument]
pub(crate) async fn query(
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
pub(crate) async fn deploy(
    Path(name): Path<String>,
    Query(query): Query<DeployQuery>,
    AuthState(user): AuthState,
    State(state): State<AppState>,
) -> Result<Json<Value>, Response> {
    validate_allowed_app(&user, &name)?;
    validate_allowed_image(&user, &query.image)?;

    let namespace = query
        .namespace
        .unwrap_or_else(|| state.kube_client.default_namespace().to_string());
    let deployment_api: Api<Deployment> = Api::namespaced(state.kube_client, &namespace);

    let container_name = query.container.as_deref().unwrap_or(&name);
    match patch_deployment_image(deployment_api, &name, &query.image, container_name).await {
        Ok(patched) => Ok(Json(deployment_to_json(patched))),
        Err(err) => {
            error!("Failed to patch deployment: {err}");
            match err {
                KubeError::Api(response) => match StatusCode::from_u16(response.code) {
                    Ok(status_code) => Err((status_code, response.message).into_response()),
                    Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
                },
                _ => Err(StatusCode::INTERNAL_SERVER_ERROR.into_response()),
            }
        }
    }
}

fn validate_allowed_app(user: &User, name: &str) -> Result<(), Response> {
    if user.allowed_apps.iter().any(|s| s == name) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN.into_response())
    }
}

fn validate_allowed_image(user: &User, image: &str) -> Result<(), Response> {
    if let Some((image_name, _)) = image.rsplit_once(':') {
        if user.allowed_images.iter().any(|s| s == image_name) {
            return Ok(());
        }
    }

    let pair = (
        StatusCode::FORBIDDEN,
        Json(json!({
            "status": StatusCode::FORBIDDEN.as_u16(),
            "error": "only allowed images can be deployed",
            "allowed_images": user.allowed_images,
        })),
    );
    Err(pair.into_response())
}

async fn patch_deployment_image(
    deployment_api: Api<Deployment>,
    deployment_name: &str,
    image: &str,
    container_name: &str,
) -> Result<Deployment, KubeError> {
    let patch = json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "spec": {
            "template": {
                "spec": {
                    "containers": [{
                        "name": container_name,
                        "image": image,
                        "imagePullPolicy": "IfNotPresent"
                    }]
                }
            }
        }
    });
    deployment_api
        .patch(deployment_name, &PATCH_PARAMS, &Patch::Strategic(patch))
        .await
}

async fn get_deployment(api: &Api<Deployment>, name: &str) -> Result<Deployment, Response> {
    api.get(name).await.map_err(|err| {
        error!("Failed to query kubernetes cluster. {err}");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })
}

fn deployment_to_json(mut deployment: Deployment) -> Value {
    deployment.metadata.managed_fields = None;
    json!(deployment)
}
