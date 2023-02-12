use crate::auth::{AuthState, User};
use crate::models::AppState;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use http::StatusCode;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Patch, PatchParams, ValidationDirective};
use kube::{Api, Error as KubeError};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::instrument;

#[derive(Debug, Deserialize)]
pub(crate) struct DeployQuery {
    image: String,
}

#[instrument]
pub(crate) async fn query(
    Path(name): Path<String>,
    AuthState(user): AuthState,
    State(state): State<AppState>,
) -> anyhow::Result<Json<Value>, Response> {
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
) -> anyhow::Result<Json<Value>, Response> {
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

fn validate_allowed_app(user: &User, name: &String) -> anyhow::Result<(), Response> {
    if user.allowed_apps.contains(&name) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN.into_response())
    }
}

fn validate_allowed_image(user: &User, image: &str) -> anyhow::Result<(), Response> {
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

async fn patch_deployment_image(
    deployment_api: Api<Deployment>,
    name: &str,
    image: &str,
) -> anyhow::Result<Deployment, KubeError> {
    let params = PatchParams {
        dry_run: false,
        force: true,
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

async fn get_deployment(api: &Api<Deployment>, name: &str) -> anyhow::Result<Deployment, Response> {
    api.get(name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn deployment_to_json(mut deployment: Deployment) -> Value {
    deployment.metadata.managed_fields = None;
    json!(deployment)
}
