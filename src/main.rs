use axum::extract::Query;
use axum::{extract::Path, http::StatusCode, routing::get, Json, Router};
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Patch, PatchParams, ValidationDirective};
use kube::{
    api::{Api, ListParams},
    Client, Config,
};
use serde::Deserialize;
use serde_json::json;
use std::error::Error;
use std::net::SocketAddr;
use tracing::debug;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new().route("/deploy/:id", get(deploy));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct DeployQuery {
    image: Option<String>,
}

async fn deploy(
    Path(id): Path<String>,
    Query(query): Query<DeployQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Infer the runtime environment and try to create a Kubernetes Client
    let cfg = Config::infer().await.unwrap();
    let client = Client::try_from(cfg).unwrap();
    let deployment_api: Api<Deployment> = Api::default_namespaced(client);
    let Ok(deployments) = deployment_api.list(&ListParams {
        label_selector: Some(format!("app={id}")),
        ..Default::default()
    }).await else {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({}))));
    };
    if let Some(image) = query.image {
        match deployments.items.first() {
            None => {}
            Some(deployment) => {
                if let Some(name) = deployment.metadata.name.as_ref() {
                    let params = PatchParams {
                        dry_run: false,
                        force: true,
                        field_manager: Some("kube-environment".into()),
                        field_validation: Some(ValidationDirective::Strict),
                    };
                    let patch1 = json!({
                        "apiVersion": "apps/v1",
                        "kind": "Deployment",
                        "spec": {
                            "template": {
                                "spec": {
                                    "containers": [
                                        {
                                            "name": name,
                                            "image": image,
                                            "imagePullPolicy": "IfNotPresent"
                                        }
                                    ]
                                }
                            }
                        }
                    });
                    let patch = Patch::Apply(&patch1);
                    let result = deployment_api.patch(name, &params, &patch).await;
                    return match &result {
                        Ok(patched) => {
                            let body: serde_json::Value = json!(patched);
                            Ok(Json(body))
                        }
                        Err(e) => {
                            tracing::error!(error =?e, "failed to patch deployment");
                            let status_code = match e {
                                kube::Error::Api(ae) => StatusCode::from_u16(ae.code)
                                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                                _ => StatusCode::INTERNAL_SERVER_ERROR,
                            };
                            return Err((status_code, Json(json!({"error": e.to_string()}))));
                        }
                    };
                }
            }
        }
    }

    let Ok(deployments) = deployment_api.list(&ListParams {
        label_selector: Some(format!("app={id}")),
        ..Default::default()
    }).await else {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(json!({}))));
    };
    let body: serde_json::Value = deployments.iter().map(|p| json!(p)).collect();
    Ok(Json(body))
}
