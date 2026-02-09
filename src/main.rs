use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, Path, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use tower_http::services::ServeFile;

// ... (existing code)

/// Handles video streaming requests.
///
/// This function initiates a transcoding stream for the requested media ID using FFmpeg.
/// It returns a chunked response of MPEG-TS data.
///
/// # Arguments
///
/// * `state` - The shared application state to look up the media file path.
/// * `id` - The ID of the media item to stream.
/// * `headers` - Request headers to extract User-Agent.
async fn stream_handler(
    State(state): State<SharedState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    req: Request,
) -> Result<Response, AppError> {
    let item = {
        let read = state
            .read()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("Lock poison")))?;
        read.library.get(&id).cloned()
    };

    // User Agent Logic
    let user_agent = headers
        .get("User-Agent")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    debug!("User Agent: {}", user_agent);

    // Detect Renderer
    let renderer = {
        let state_read = state
            .read()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("Lock poison")))?;

        // Priority 1: Check Registry
        if let Some(r) = state_read.registry.get_renderer(&addr.ip()) {
            debug!(
                "Registry Detected Renderer (IP {}): {}",
                addr.ip(),
                r.renderer_name
            );
            r
        } else {
            // Priority 2: Fallback to User-Agent
            let mut renderer = &state_read.default_renderer;
            for r in &state_read.renderers {
                if r.is_match(user_agent) {
                    renderer = r;
                    break;
                }
            }
            renderer.clone()
        }
    };
    debug!("Stream Detected Renderer: {}", renderer.renderer_name);
    if let Some(item) = item {
        info!("Streaming: {}", item.title);

        let is_transcoded = !renderer.is_compatible(&item);
        let mime_type = if is_transcoded {
            "video/mpeg".to_string()
        } else {
            item.mime_type.clone()
        };

        // Use dlna_helper to get features based on renderer config
        let features =
            upnp::dlna_helper::get_dlna_org_pn_flags(&mime_type, &renderer, is_transcoded);

        if renderer.is_compatible(&item) {
            info!("Direct Play Supported. Serving raw file with Range support.");

            // ServeFile handles Range requests automatically
            match ServeFile::new(&item.path).try_call(req).await {
                Ok(response) => {
                    // We need to inject DLNA headers into the response from ServeFile
                    let (mut parts, body) = response.into_parts();
                    parts
                        .headers
                        .insert("contentFeatures.dlna.org", features.parse().unwrap());
                    parts
                        .headers
                        .insert("transferMode.dlna.org", "Streaming".parse().unwrap());
                    parts
                        .headers
                        .insert("realTimeInfo.dlna.org", "DLNA.ORG_TLAG=*".parse().unwrap());

                    Ok(Response::from_parts(parts, Body::new(body)))
                }
                Err(e) => {
                    error!("ServeFile error: {}", e);
                    Err(AppError::Internal(anyhow::anyhow!("ServeFile failed")))
                }
            }
        } else {
            if renderer.is_compatible(&item) {
                info!("Item compatible but forcing transcode for debug.");
            }
            info!("Transcoding to MPEG-TS.");
            // Transcode
            let stream = transcoding::Transcoder::spawn_stream(&item.path, 0)
                .map_err(|e| AppError::Internal(e.into()))?;

            Ok(Response::builder()
                .header("Content-Type", mime_type)
                .header("Transfer-Encoding", "chunked") // Streaming
                .header("contentFeatures.dlna.org", features)
                .header("transferMode.dlna.org", "Streaming")
                .body(Body::from_stream(stream))
                .unwrap())
        }
    } else {
        Err(AppError::NotFound("Media Not Found".to_string()))
    }
}
use clap::Parser;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tracing::{debug, error, info, warn};

pub mod configuration; // Make configuration public
mod discovery;
mod error;
mod library;
mod logging;
mod state;
mod transcoding;
mod upnp;

use configuration::renderer::RendererConfiguration;
use error::AppError;
use state::{AppState, SharedState};

/// Command line arguments
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Log level (error, warn, info, debug, trace)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

/// The main entry point of the application.
///
/// This function initializes the application state, starts the SSDP service,
/// initiates the file system watcher, and sets up the HTTP server with Axum routes.
#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Set up file appender (daily rotation)
    let file_appender = tracing_appender::rolling::daily("logs", "server.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Set up tracing with environment filter from args
    // We use a Registry to combine stdout and file layers
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level));

    let stdout_layer = tracing_subscriber::fmt::layer().pretty();
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false); // Disable colors for file log

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    if args.log_level == "debug" {
        debug!("Debug mode enabled");
    }

    // Spawn log cleanup task
    logging::spawn_log_cleanup_task(std::path::PathBuf::from("logs"));

    // Load configurations
    let config_dir = std::path::PathBuf::from("config");
    let mut renderers = Vec::new();
    let mut default_renderer = RendererConfiguration::default();

    // Ensure config directory exists
    if config_dir.exists() && config_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("conf") {
                    match RendererConfiguration::load(&path) {
                        Ok(config) => {
                            // info!("Loaded renderer config: {}", config.renderer_name);
                            if path.file_name().and_then(|n| n.to_str())
                                == Some("DefaultRenderer.conf")
                            {
                                default_renderer = config.clone();
                            }
                            renderers.push(config);
                        }
                        Err(e) => error!("Failed to load config {:?}: {}", path, e),
                    }
                }
            }
        }
    } else {
        warn!("Config directory not found, using defaults");
    }

    // Sort renderers by loading priority (descending)
    renderers.sort_by(|a, b| b.loading_priority.cmp(&a.loading_priority));

    // Configuration
    // Fixed UUID for testing to prevent duplicates
    let uuid = "12345678-1234-1234-1234-123456789abc".to_string();
    // uuid::Uuid::new_v4().to_string();
    // "12345678-1234-1234-1234-123456789abc".to_string();

    let port = 3000;
    let content_dir = std::path::PathBuf::from("media"); // Default media folder

    // Ensure media directory exists
    if let Err(e) = tokio::fs::create_dir_all(&content_dir).await {
        error!("Failed to create media directory: {}", e);
        return;
    }

    let state = Arc::new(RwLock::new(AppState::new(
        uuid.clone(),
        content_dir.clone(),
        renderers,
        default_renderer,
    )));

    // Start SSDP
    let ssdp = discovery::ssdp::SSDPService::new(uuid.clone(), port, state.clone());
    ssdp.spawn().await;

    // Start Watcher
    let watcher_state = state.clone();
    let watcher_dir = content_dir.clone();
    tokio::spawn(async move {
        let watcher = library::watcher::LibraryWatcher::new(watcher_state, watcher_dir);
        watcher.start().await;
    });

    // Router
    let app = Router::new()
        .route(
            "/services/ContentDirectory/control",
            post(upnp::soap::content_directory_control),
        )
        .route(
            "/services/ConnectionManager/control",
            post(upnp::soap::connection_manager_control),
        )
        .route(
            "/services/X_MS_MediaReceiverRegistrar/control",
            post(upnp::soap::media_receiver_registrar_control),
        )
        .route("/stream/{id}", get(stream_handler))
        .route("/description.xml", get(description_handler))
        // SCPD Routes
        .route(
            "/services/ContentDirectory/scpd.xml",
            get(scpd_content_directory),
        )
        .route(
            "/services/ConnectionManager/scpd.xml",
            get(scpd_connection_manager),
        )
        .route(
            "/services/X_MS_MediaReceiverRegistrar/scpd.xml",
            get(scpd_media_receiver_registrar),
        )
        // Serve icons
        .route("/icon-256.png", get(serve_icon_png))
        .route("/icon-120.jpg", get(serve_icon_jpg))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Server listening on {}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind to port {}: {}", port, e);
            return;
        }
    };

    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        error!("Server error: {}", e);
    }
}

/// Handles requests for the `description.xml` file.
///
/// This file is critical for UPnP/DLNA device discovery. It returns an XML description
/// of the device, its services, and capabilities, reading the template from `resources/description.xml`
/// and injecting the device's UUID.
///
/// # Arguments
///
/// * `state` - The shared application state containing the device UUID.
async fn description_handler(State(state): State<SharedState>) -> impl IntoResponse {
    // Graceful unwrap for Read Lock
    let uuid = match state.read() {
        Ok(guard) => guard.uuid.clone(),
        Err(e) => {
            error!("State read lock poisoned: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal State Error").into_response();
        }
    };

    let _local_ip = local_ip_address::local_ip()
        .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));

    match tokio::fs::read_to_string("resources/description.xml").await {
        Ok(template) => {
            let xml = template.replace("{}", &uuid);
            ([(header::CONTENT_TYPE, "text/xml")], xml).into_response()
        }
        Err(e) => {
            error!("Failed to read description.xml: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Serves the 256x256 PNG device icon.
async fn serve_icon_png() -> impl IntoResponse {
    serve_icon("icon/icon-256.png", "image/png").await
}

/// Serves the 12x120 JPEG device icon.
async fn serve_icon_jpg() -> impl IntoResponse {
    serve_icon("icon/icon-120.jpg", "image/jpg").await
}

/// Generic helper function to serve icon files.
///
/// # Arguments
///
/// * `path` - The file system path to the icon image.
/// * `mime` - The MIME type of the image (e.g., "image/png").
async fn serve_icon(path: &str, mime: &str) -> Response {
    match tokio::fs::read(path).await {
        Ok(data) => ([(header::CONTENT_TYPE, mime)], Body::from(data)).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Serves the SCPD (Service Control Protocol Definition) for the Content Directory service.
async fn scpd_content_directory() -> impl IntoResponse {
    serve_xml("resources/content_directory_scpd.xml").await
}

/// Serves the SCPD for the Connection Manager service.
async fn scpd_connection_manager() -> impl IntoResponse {
    serve_xml("resources/connection_manager_scpd.xml").await
}

/// Serves the SCPD for the Media Receiver Registrar service.
async fn scpd_media_receiver_registrar() -> impl IntoResponse {
    serve_xml("resources/media_receiver_registrar_scpd.xml").await
}

/// Generic helper function to serve XML resource files.
///
/// # Arguments
///
/// * `path` - The path to the XML file in the `resources` directory.
async fn serve_xml(path: &str) -> Response {
    match tokio::fs::read_to_string(path).await {
        Ok(xml) => ([(header::CONTENT_TYPE, "text/xml")], xml).into_response(),
        Err(e) => {
            error!("Failed to read {}: {}", path, e);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}
