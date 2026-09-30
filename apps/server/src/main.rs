use std::{net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let address = std::env::var("RETIREMENT_CALCULATOR_BIND")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse::<SocketAddr>()?;
    let assets = std::env::var_os("RETIREMENT_CALCULATOR_PUBLIC_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../public"),
        PathBuf::from,
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("retirement calculator listening on http://{address}");
    axum::serve(
        listener,
        server::router(assets, uuid::Uuid::new_v4().to_string()),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
