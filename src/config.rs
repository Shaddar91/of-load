//Service settings read from the environment.

use std::env;
use std::path::PathBuf;
use std::str::FromStr;

use axum::http::HeaderValue;

pub struct Config {
    pub port: u16,
    pub of_api_url: String,
    pub cors_allowed_origins: Vec<HeaderValue>,
    pub levels_file: Option<PathBuf>,
    pub max_inflight: usize,
    pub max_cpu_ms: u64,
    pub max_mem_mib: u64,
    pub auth_cache_seconds: u64,
    pub pod_name: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            port: number("PORT", 8000)?,
            of_api_url: text("OF_API_URL").unwrap_or_else(|| "http://api:8000".to_owned()),
            cors_allowed_origins: origins(
                text("CORS_ALLOWED_ORIGINS")
                    .as_deref()
                    .unwrap_or("http://localhost:5173"),
            )?,
            levels_file: text("STRESS_LEVELS_FILE").map(PathBuf::from),
            max_inflight: number("STRESS_MAX_INFLIGHT", 4)?,
            max_cpu_ms: number("STRESS_MAX_CPU_MS", 2000)?,
            max_mem_mib: number("STRESS_MAX_MEM_MIB", 256)?,
            auth_cache_seconds: number("AUTH_CACHE_SECONDS", 30)?,
            pod_name: text("POD_NAME")
                .or_else(|| text("HOSTNAME"))
                .unwrap_or_else(|| "unknown".to_owned()),
        })
    }
}

fn text(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn number<T: FromStr>(name: &str, default: T) -> Result<T, String> {
    match text(name) {
        Some(raw) => raw
            .parse()
            .map_err(|_| format!("{name}={raw} is not a valid number")),
        None => Ok(default),
    }
}

fn origins(raw: &str) -> Result<Vec<HeaderValue>, String> {
    raw.split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            HeaderValue::from_str(origin)
                .map_err(|_| format!("CORS_ALLOWED_ORIGINS: {origin} is not a valid origin"))
        })
        .collect()
}
