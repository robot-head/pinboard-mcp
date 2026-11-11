mod collab;
mod export;
mod http;
mod mail;
mod mcp;
mod models;
mod saml;
mod security;
mod store;

use clap::Parser;
use rmcp::ServiceExt;
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    version,
    about = "A persistent visual canvas for Claude, Codex, and humans"
)]
struct Args {
    /// Run HTTP/UI + Streamable HTTP MCP only. Default also serves stdio MCP.
    #[arg(long)]
    serve: bool,
    #[arg(long, env = "PINBOARD_PORT", default_value_t = 4127)]
    port: u16,
    /// Public binding requires SSO and an HTTPS public URL.
    #[arg(long, default_value = "127.0.0.1")]
    bind: IpAddr,
    #[arg(long, env = "PINBOARD_PUBLIC_URL")]
    public_url: Option<String>,
    #[arg(long, env = "PINBOARD_DB", default_value = "data/pinboard.db")]
    db: PathBuf,
    /// Remote Turso Cloud URL. Auth token is read from TURSO_AUTH_TOKEN.
    #[arg(long, env = "TURSO_DATABASE_URL")]
    turso_url: Option<String>,
    #[arg(long, env = "PINBOARD_DIST", default_value = "crates/web/dist")]
    dist: PathBuf,
    /// Do not create the welcome canvas in a new database.
    #[arg(long)]
    no_demo: bool,
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pinboard_mcp=info".into()),
        )
        .init();
    let args = Args::parse();
    let remote = args
        .turso_url
        .map(|url| std::env::var("TURSO_AUTH_TOKEN").map(|token| (url, token)))
        .transpose()
        .map_err(|_| anyhow::anyhow!("TURSO_AUTH_TOKEN is required with TURSO_DATABASE_URL"))?;
    let store = store::Store::open(&args.db, remote, !args.no_demo).await?;
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(args.bind, args.port)).await?;
    let port = listener.local_addr()?.port();
    let host = match args.bind {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    let url = args
        .public_url
        .unwrap_or_else(|| format!("http://{host}:{port}"))
        .trim_end_matches('/')
        .to_string();
    let parsed = url::Url::parse(&url)?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https")
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none()
            && parsed.path() == "/",
        "PINBOARD_PUBLIC_URL must be an HTTP(S) origin without credentials, path, or query"
    );
    let url = parsed.origin().ascii_serialization();
    let security = security::Security::from_env(store.clone(), url.clone()).await?;
    let public_host = parsed.host_str().unwrap_or("");
    let local_origin = matches!(public_host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
    anyhow::ensure!(
        security.configured() || (args.bind.is_loopback() && local_origin),
        "Public hosting requires OIDC or SAML configuration"
    );
    anyhow::ensure!(
        (args.bind.is_loopback() && local_origin) || parsed.scheme() == "https",
        "Public hosting requires an HTTPS public URL and TLS termination"
    );
    let mailer = mail::Mailer::from_env()?;
    let stdio_user = if !args.serve && security.configured() {
        let token=std::env::var("PINBOARD_STDIO_TOKEN").map_err(|_|anyhow::anyhow!("Use --serve for HTTP MCP, or set PINBOARD_STDIO_TOKEN to a revocable personal API token"))?;
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse()?,
        );
        Some((security.authenticate(&headers).await?.user, headers))
    } else {
        None
    };
    tracing::info!(url=%url,mcp=%format!("{url}/mcp"),"Pinboard is ready");
    if !args.dist.join("index.html").exists() {
        tracing::warn!("UI bundle missing. Run: cd crates/web && trunk build --release");
    }
    let app = http::router_configured(
        store.clone(),
        args.dist,
        url.clone(),
        port,
        security.clone(),
        mailer,
    );
    let web = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
    });
    if args.serve {
        web.await??;
    } else {
        let server = if let Some((user, headers)) = stdio_user {
            mcp::CanvasMcp::new_authenticated(store, url, user, security, headers)
        } else {
            mcp::CanvasMcp::new_user(store, url, collab::local_user())
        };
        let service = server.serve(rmcp::transport::stdio()).await?;
        tokio::select! {result=service.waiting()=>{result?;},_ = tokio::signal::ctrl_c()=>{}}
        web.abort();
    }
    Ok(())
}
