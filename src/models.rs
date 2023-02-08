use anyhow::Result;
use serde::Deserialize;
use std::fmt::{Debug, Formatter};

#[derive(Clone, Deserialize)]
pub struct Config {
    pub openid_url: url::Url,
    pub openid_realm: String,
}

impl Debug for Config {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("openid_url", &self.openid_url.as_str())
            .field("openid_realm", &self.openid_realm)
            .finish()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub kube_client: kube::Client,
    pub reqwest_client: reqwest::Client,
}

impl Debug for AppState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl AppState {
    pub async fn from_env() -> Result<Self> {
        Ok(AppState {
            config: envy::from_env()?,
            kube_client: kube::Client::try_default().await?,
            reqwest_client: reqwest::Client::new(),
        })
    }
}
