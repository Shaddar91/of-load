//Shared state: settings, levels, burn permits, the of-api client, the token cache and the pod name.

use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Url};
use tokio::sync::Semaphore;

use crate::auth::TokenCache;
use crate::config::Config;
use crate::levels::Levels;

pub struct AppState {
    pub(crate) config: Config,
    pub(crate) levels: Levels,
    pub(crate) burns: Arc<Semaphore>,
    pub(crate) client: Client,
    pub(crate) cache: TokenCache,
    pub(crate) me_url: Url,
}

impl AppState {
    pub fn from_env() -> Result<Self, String> {
        let config = Config::from_env()?;
        let levels = Levels::load(
            config.levels_file.as_deref(),
            config.max_cpu_ms,
            config.max_mem_mib,
        )?;
        let base = config.of_api_url.trim_end_matches('/');
        let me_url = Url::parse(&format!("{base}/api/v1/me"))
            .map_err(|error| format!("OF_API_URL={base}: {error}"))?;
        Ok(Self {
            burns: Arc::new(Semaphore::new(config.max_inflight)),
            cache: TokenCache::new(Duration::from_secs(config.auth_cache_seconds)),
            config,
            levels,
            client: Client::new(),
            me_url,
        })
    }

    pub fn port(&self) -> u16 {
        self.config.port
    }

    pub fn pod(&self) -> &str {
        &self.config.pod_name
    }
}
