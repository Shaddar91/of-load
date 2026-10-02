//of-load entrypoint: JSON logs to stdout, listen on 0.0.0.0:$PORT, stop on SIGTERM or SIGINT.

use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;

use of_load::{AppState, router};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let state = match AppState::from_env() {
        Ok(state) => state,
        Err(reason) => {
            error!(%reason, "startup failed");
            return ExitCode::FAILURE;
        }
    };
    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, state.port()));
    let listener = match TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(reason) => {
            error!(%address, %reason, "bind failed");
            return ExitCode::FAILURE;
        }
    };
    info!(%address, pod = state.pod(), "listening");
    let served = axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown())
        .await;
    if let Err(reason) = served {
        error!(%reason, "server failed");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn shutdown() {
    let terminate = async {
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = terminate => {}
    }
    info!("shutting down");
}
