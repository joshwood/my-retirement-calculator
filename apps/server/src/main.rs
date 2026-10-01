use std::{env::VarError, path::PathBuf};

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();
    let port = match std::env::var("PORT") {
        Ok(port) => Some(port),
        Err(VarError::NotPresent) => None,
        Err(error) => return Err(error.into()),
    };
    let address = server::bind_address(port.as_deref())?;
    let assets = std::env::var_os("RETIREMENT_CALCULATOR_PUBLIC_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../public"),
        PathBuf::from,
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(event = "server_started", bind = %address);
    axum::serve(
        listener,
        server::router(assets, uuid::Uuid::new_v4().to_string()),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(event = "shutdown_signal_error", signal = "ctrl_c", %error);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::error!(event = "shutdown_signal_error", signal = "sigterm", %error);
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(unix)]
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    #[cfg(not(unix))]
    ctrl_c.await;

    tracing::info!(event = "shutdown_signal_received");
}
