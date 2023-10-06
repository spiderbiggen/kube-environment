use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::StatusCode;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Patch, PatchParams, ValidationDirective};
use kube::{Api, Error as KubeError};
use lazy_static::lazy_static;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::instrument;

use crate::auth::{AuthState, User};
use crate::models::AppState;

const FIELD_MANAGER: &str = "kube-environment";
lazy_static! {
    static ref PATCH_PARAMS: PatchParams = PatchParams {
        dry_run: false,
        force: true,
        field_manager: Some(String::from(FIELD_MANAGER)),
        field_validation: Some(ValidationDirective::Strict),
    };
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeployQuery {
    image: String,
    container: Option<String>,
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

    let deployment_api: Api<Deployment> = Api::default_namespaced(state.kube_client);

    let container_name = query.container.as_ref().unwrap_or(&name);
    match patch_deployment_image(deployment_api, &name, &query.image, container_name).await {
        Ok(patched) => Ok(Json(deployment_to_json(patched))),
        Err(e) => {
            tracing::error!(error =%e, "failed to patch deployment");
            Err(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

fn validate_allowed_app(user: &User, name: &String) -> Result<(), Response> {
    if user.allowed_apps.contains(name) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN.into_response())
    }
}

fn validate_allowed_image(user: &User, image: &str) -> Result<(), Response> {
    let option = image.rsplit_once(':');
    if let Some((image_name, _)) = option {
        if user.allowed_images.iter().any(|s| s == image_name) {
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
        .patch(deployment_name, &PATCH_PARAMS, &Patch::Apply(patch))
        .await
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
