use crate::{error::AppError, state::SharedState};
use axum::{body::Body, extract::State, http::HeaderMap, response::Response};
use tracing::{debug, error};

/// Handles SOAP actions for the ConnectionManager service.
///
/// Only supports `GetProtocolInfo`, which is required for DLNA players to know
/// what media formats the server supports.
///
/// # Arguments
///
/// * `headers` - The HTTP headers containing the `SOAPACTION`.
pub async fn connection_manager_control(
    State(_state): State<SharedState>,
    headers: HeaderMap,
    _body: String,
) -> Result<Response, AppError> {
    let soap_action = headers
        .get("SOAPACTION")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    debug!("CM Received SOAP Action: {}", soap_action);

    if soap_action.contains("GetProtocolInfo") {
        Ok(handle_get_protocol_info().await?)
    } else {
        error!("Unknown CM SOAP Action: {}", soap_action);
        Err(AppError::BadRequest(
            "Unknown or Unsupported SOAP Action".to_string(),
        ))
    }
}

/// Handles SOAP actions for the X_MS_MediaReceiverRegistrar service.
///
/// Supports `IsAuthorized` and `IsValidated`. These are often required by Xbox
/// and some smart TVs to "trust" the media server.
pub async fn media_receiver_registrar_control(
    State(_state): State<SharedState>,
    headers: HeaderMap,
    _body: String,
) -> Result<Response, AppError> {
    let soap_action = headers
        .get("SOAPACTION")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    debug!("MRR Received SOAP Action: {}", soap_action);

    if soap_action.contains("IsAuthorized") || soap_action.contains("IsValidated") {
        Ok(handle_is_authorized().await?)
    } else {
        error!("Unknown MRR SOAP Action: {}", soap_action);
        Err(AppError::BadRequest(
            "Unknown or Unsupported SOAP Action".to_string(),
        ))
    }
}

/// Generates the DIDL-Lite response for `GetProtocolInfo`.
///
/// Returns a hardcoded list of supported MIME types (Source) and an empty list of Sinks.
/// Includes DLNA feature flags for better device compatibility.
async fn handle_get_protocol_info() -> Result<Response, AppError> {
    // DLNA.ORG_OP: 01 = Range seek (bytes), 10 = Time seek, 11 = Both
    // DLNA.ORG_CI: 0 = Not transcoded, 1 = Transcoded
    // DLNA.ORG_FLAGS: Standard streaming flags
    const DLNA_FLAGS: &str =
        "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000";

    // Comprehensive protocol info with DLNA feature flags for maximum device compatibility
    let source = format!(
        "http-get:*:video/mpeg:DLNA.ORG_PN=MPEG_PS_PAL;{flags},\
         http-get:*:video/mpeg:DLNA.ORG_PN=MPEG_PS_NTSC;{flags},\
         http-get:*:video/vnd.dlna.mpeg-tts:DLNA.ORG_PN=AVC_TS_NA_ISO;{flags},\
         http-get:*:video/mp2t:DLNA.ORG_PN=AVC_TS_MP_HD_AAC_ISO;{flags},\
         http-get:*:video/mp4:DLNA.ORG_PN=AVC_MP4_BL_CIF15_AAC_520;{flags},\
         http-get:*:video/mp4:DLNA.ORG_PN=AVC_MP4_MP_SD_AAC_MULT5;{flags},\
         http-get:*:video/x-matroska:DLNA.ORG_PN=AVC_MKV_MP_HD_AAC_MULT5;{flags},\
         http-get:*:video/x-matroska:*,\
         http-get:*:audio/mpeg:DLNA.ORG_PN=MP3;{flags},\
         http-get:*:audio/mp4:DLNA.ORG_PN=AAC_ISO;{flags},\
         http-get:*:audio/L16:DLNA.ORG_PN=LPCM;{flags}",
        flags = DLNA_FLAGS
    );

    let sink = "";

    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
    <s:Body>
        <u:GetProtocolInfoResponse xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1">
            <Source>{}</Source>
            <Sink>{}</Sink>
        </u:GetProtocolInfoResponse>
    </s:Body>
</s:Envelope>"#,
        source, sink
    );

    Ok(Response::builder()
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("Server", "Rust-DLNA/1.0 UPnP/1.0")
        .body(Body::from(body))
        .unwrap())
}

/// Generates the response for `IsAuthorized`.
///
/// Always returns `1` (Authorized) to allow any device to connect.
async fn handle_is_authorized() -> Result<Response, AppError> {
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
    <s:Body>
        <u:IsAuthorizedResponse xmlns:u="urn:microsoft.com:service:X_MS_MediaReceiverRegistrar:1">
            <Result>1</Result>
        </u:IsAuthorizedResponse>
    </s:Body>
</s:Envelope>"#;

    Ok(Response::builder()
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("Server", "Rust-DLNA/1.0 UPnP/1.0")
        .body(Body::from(body))
        .unwrap())
}

/// Handles SOAP actions for the ContentDirectory service.
///
/// The primary action is `Browse`, which clients use to navigate the folder structure
/// and list media items.
pub async fn content_directory_control(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, AppError> {
    let soap_action = headers
        .get("SOAPACTION")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    let user_agent = headers
        .get("User-Agent")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    debug!("Received SOAP Action: {}", soap_action);
    debug!("User-Agent: {}", user_agent);

    if soap_action.contains("Browse") {
        Ok(handle_browse(state, &body, user_agent).await?)
    } else {
        Err(AppError::BadRequest(
            "Unknown or Unsupported SOAP Action".to_string(),
        ))
    }
}

/// Handles the `Browse` SOAP action.
///
/// It generates a DIDL-Lite XML document listing all media files currently in the
/// server's library. Currently, it supports a flat list (DirectChildren of Root).
///
/// # Arguments
///
/// * `state` - The shared application state containing the media library.
/// * `_body` - The request body.
/// * `user_agent` - The User-Agent header from the request.
async fn handle_browse(
    state: SharedState,
    body: &str,
    user_agent: &str,
) -> Result<Response, AppError> {
    // Simple parsing for ObjectID and BrowseFlag
    // Ideally use quick-xml, but string contains is faster for this specific case
    let object_id = if let Some(start) = body.find("<ObjectID>") {
        if let Some(end) = body[start..].find("</ObjectID>") {
            &body[start + 10..start + end]
        } else {
            "0"
        }
    } else {
        "0"
    };

    let browse_flag = if let Some(start) = body.find("<BrowseFlag>") {
        if let Some(end) = body[start..].find("</BrowseFlag>") {
            &body[start + 12..start + end]
        } else {
            "BrowseDirectChildren"
        }
    } else {
        "BrowseDirectChildren"
    };

    debug!("Browse: ObjectID={}, Flag={}", object_id, browse_flag);

    // Lock safety
    let state_read = state
        .read()
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Lock poison")))?;
    let library = &state_read.library;
    let system_update_id = state_read.system_update_id;

    // Detect Renderer
    let mut renderer = &state_read.default_renderer;
    let mut matched = false;
    for r in &state_read.renderers {
        if r.is_match(user_agent) {
            renderer = r;
            debug!(
                "Detected Renderer: '{}' for User-Agent: '{}'",
                r.renderer_name, user_agent
            );
            matched = true;
            break;
        }
    }

    if !matched {
        debug!(
            "No renderer matched for User-Agent: '{}'. Using Default: '{}'",
            user_agent, renderer.renderer_name
        );
    }

    let mut didl_lite = String::from(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/">"#,
    );

    let mut number_returned = 0;
    let mut total_matches = 0;

    if object_id == "0" && browse_flag == "BrowseMetadata" {
        // Return Root Container Metadata
        didl_lite.push_str(
            r#"<container id="0" parentID="-1" restricted="1" childCount="1">
                <dc:title>Root</dc:title>
                <upnp:class>object.container.storageFolder</upnp:class>
            </container>"#,
        );
        number_returned = 1;
        total_matches = 1;
    } else if object_id == "0" && browse_flag == "BrowseDirectChildren" {
        // Return All Items
        let local_ip = local_ip_address::local_ip()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)));
        let http_port = 3000;

        for item in library.values() {
            let url = format!("http://{}:{}/stream/{}", local_ip, http_port, item.id);

            // Check compatibility
            let is_transcoded = !renderer.is_compatible(item);
            let (mime_type, features) = if is_transcoded {
                (
                    "video/mpeg",
                    crate::upnp::dlna_helper::get_dlna_org_pn_flags("video/mpeg", renderer, true),
                )
            } else {
                let mime = item.mime_type.as_str();
                (
                    mime,
                    crate::upnp::dlna_helper::get_dlna_org_pn_flags(mime, renderer, false),
                )
            };

            let protocol_info = if features.is_empty() {
                format!("http-get:*:{}", mime_type)
            } else {
                format!("http-get:*:{}:{}", mime_type, features)
            };
            debug!("[handle_browse]Protocol info: {}", protocol_info);
            // debug!("URL: {}", url);
            // debug!("Size: {:#?}", item);

            didl_lite.push_str(&format!(
                r#"<item id="{}" parentID="0" restricted="1">
                    <dc:title>{}</dc:title>
                    <upnp:class>object.item.videoItem</upnp:class>
                    <res protocolInfo="{}" size="{}">{}</res>
                </item>"#,
                item.id, item.title, protocol_info, item.size, url
            ));
        }
        number_returned = library.len();
        total_matches = library.len();
    }
    // Else: Unknown ObjectID or Flag, return empty (already default)

    didl_lite.push_str("</DIDL-Lite>");

    // Escape basic XML characters in the DIDL-Lite payload
    let didl_escaped = didl_lite
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;");

    let response_body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
    <s:Body>
        <u:BrowseResponse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1">
            <Result>{}</Result>
            <NumberReturned>{}</NumberReturned>
            <TotalMatches>{}</TotalMatches>
            <UpdateID>{}</UpdateID>
        </u:BrowseResponse>
    </s:Body>
</s:Envelope>"#,
        didl_escaped, number_returned, total_matches, system_update_id
    );

    Ok(Response::builder()
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("Server", "Rust-DLNA/1.0 UPnP/1.0")
        .body(Body::from(response_body))
        .unwrap())
}
