use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use tracing::{debug, warn};

use crate::library::media_item::MediaItem;

#[derive(Debug, Clone)]
pub struct RendererSupport {
    pub formats: Vec<String>,
    pub video_codecs: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub struct RendererConfiguration {
    pub renderer_name: String,
    pub renderer_icon: String,
    pub user_agent_search: Option<String>,
    pub upnp_details_search: Option<String>,
    pub loading_priority: i32,
    pub seek_by_time: String, // true, false, exclusive
    pub default_vbv_buf_size: bool,
    pub chunked_transfer: bool,
    pub mux_non_mod4_resolution: bool,
    pub dlna_org_pn_used: bool,
    pub send_dlna_org_flags: bool,
    pub accurate_dlna_org_pn: bool,
    pub dlna_profile_changes: HashMap<String, String>,
    pub mime_types_changes: HashMap<String, String>,
    pub transcode_extensions: Vec<String>,
    pub stream_extensions: Vec<String>,
    pub supported_formats: Vec<RendererSupport>,
}

impl Default for RendererConfiguration {
    fn default() -> Self {
        Self {
            renderer_name: "Unknown Renderer".to_string(),
            renderer_icon: "unknown.png".to_string(),
            user_agent_search: None,
            upnp_details_search: None,
            loading_priority: 0,
            seek_by_time: "false".to_string(),
            default_vbv_buf_size: false,
            chunked_transfer: false,
            mux_non_mod4_resolution: false,
            dlna_org_pn_used: true,
            send_dlna_org_flags: true,
            accurate_dlna_org_pn: false,
            dlna_profile_changes: HashMap::new(),
            mime_types_changes: HashMap::new(),
            transcode_extensions: Vec::new(),
            stream_extensions: Vec::new(),
            supported_formats: Vec::new(),
        }
    }
}

impl RendererConfiguration {
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let reader = io::BufReader::new(file);
        let mut config = RendererConfiguration::default();

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "RendererName" => config.renderer_name = value.to_string(),
                    "RendererIcon" => config.renderer_icon = value.to_string(),
                    "UserAgentSearch" => {
                        if !value.is_empty() {
                            config.user_agent_search = Some(value.to_string());
                        }
                    }
                    "UpnpDetailsSearch" => {
                        if !value.is_empty() {
                            config.upnp_details_search = Some(value.to_string());
                        }
                    }
                    "LoadingPriority" => config.loading_priority = value.parse().unwrap_or(0),
                    "SeekByTime" => config.seek_by_time = value.to_string(),
                    "DefaultVBVBufSize" => {
                        config.default_vbv_buf_size = value.parse().unwrap_or(false)
                    }
                    "ChunkedTransfer" => config.chunked_transfer = value.parse().unwrap_or(false),
                    "MuxNonMod4Resolution" => {
                        config.mux_non_mod4_resolution = value.parse().unwrap_or(false)
                    }
                    "DLNAOrgPN" => config.dlna_org_pn_used = value.parse().unwrap_or(false),
                    "SendDLNAOrgFlags" => {
                        config.send_dlna_org_flags = value.parse().unwrap_or(true)
                    }
                    "AccurateDLNAOrgPN" => {
                        config.accurate_dlna_org_pn = value.parse().unwrap_or(false)
                    }
                    "DLNAProfileChanges" => {
                        for change in value.split('|') {
                            if let Some((old, new)) = change.split_once('=') {
                                config
                                    .dlna_profile_changes
                                    .insert(old.trim().to_uppercase(), new.trim().to_uppercase());
                            }
                        }
                    }
                    "MimeTypesChanges" => {
                        for change in value.split('|') {
                            if let Some((old, new)) = change.split_once('=') {
                                config
                                    .mime_types_changes
                                    .insert(old.trim().to_lowercase(), new.trim().to_lowercase());
                            }
                        }
                    }
                    "TranscodeExtensions" => {
                        config.transcode_extensions = value
                            .split(',')
                            .map(|s| s.trim().to_lowercase())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    "StreamExtensions" => {
                        config.stream_extensions = value
                            .split(',')
                            .map(|s| s.trim().to_lowercase())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    "Supported" => {
                        // Parse Supported line: f:mp4|m4v v:h264|mp4 a:aac-lc|he-aac m:video/mp4
                        let mut support = RendererSupport {
                            formats: Vec::new(),
                            video_codecs: Vec::new(),
                            audio_codecs: Vec::new(),
                            mime_type: "".to_string(),
                        };

                        // Split by whitespace
                        for part in value.split_whitespace() {
                            if let Some((prefix, content)) = part.split_once(':') {
                                let items: Vec<String> =
                                    content.split('|').map(|s| s.to_lowercase()).collect();
                                match prefix {
                                    "f" => support.formats = items,
                                    "v" => support.video_codecs = items,
                                    "a" => support.audio_codecs = items,
                                    "m" => support.mime_type = content.to_string(),
                                    _ => {} // Ignore other tags like si:
                                }
                            }
                        }
                        config.supported_formats.push(support);
                    }
                    _ => {
                        // specialized or unknown keys
                    }
                }
            }
        }
        // debug!("Loaded renderer config: {:#?}", config);
        Ok(config)
    }

    pub fn is_match(&self, user_agent: &str) -> bool {
        if let Some(ref pattern) = self.user_agent_search {
            if let Ok(re) = Regex::new(pattern) {
                return re.is_match(user_agent);
            } else {
                // Fallback to simple contains if regex is invalid (though ideally we log error)
                warn!(
                    "Invalid regex pattern for renderer {}: {}",
                    self.renderer_name, pattern
                );
                return user_agent.contains(pattern);
            }
        }
        false
    }

    pub fn match_upnp_details(&self, details: &str) -> bool {
        if let Some(ref raw_pattern) = self.upnp_details_search {
            // UMS Logic: Split by " , " and join with ".*" to allow matching multiple parts
            let parts: Vec<&str> = raw_pattern.split(" , ").collect();
            let pattern = parts.join(".*");

            // Build case-insensitive regex
            // Note: UMS replaces newlines in details with space before matching
            let clean_details = details.replace('\n', " ");

            if let Ok(re) = Regex::new(&format!("(?i){}", pattern)) {
                return re.is_match(&clean_details);
            } else {
                warn!(
                    "Invalid UPnP regex pattern for renderer {}: {}",
                    self.renderer_name, pattern
                );
                return clean_details
                    .to_lowercase()
                    .contains(&pattern.to_lowercase());
            }
        }
        false
    }

    pub fn get_dlna_profile_id(&self, profile: &str) -> String {
        self.dlna_profile_changes
            .get(&profile.to_uppercase())
            .cloned()
            .unwrap_or_else(|| profile.to_string())
    }

    /// Checks if a media item is supported by the renderer for direct play.
    pub fn is_compatible(&self, item: &MediaItem) -> bool {
        // If no supported lines are defined, default to transcode (safe)
        // OR default to compatible if empty? UMS usually transcodes unknown.
        if self.supported_formats.is_empty() {
            debug!(
                "No supported lines defined for renderer {}",
                self.renderer_name
            );
            return false;
        }

        for support in &self.supported_formats {
            let mut format_match = true;
            let mut v_match = true;
            let mut a_match = true;

            if !support.formats.is_empty() {
                if let Some(ref container) = item.container {
                    if !support.formats.contains(&container.to_lowercase()) {
                        debug!("Container {} not supported", container);
                        format_match = false;
                    }
                } else {
                    // If item has no container info, we can't be sure, so assume no match
                    debug!("Item has no container info, assuming no match");
                    format_match = false;
                }
            }

            if !support.video_codecs.is_empty()
                && let Some(ref v_codec) = item.video_codec
                    && !support.video_codecs.contains(&v_codec.to_lowercase()) {
                        debug!("Video codec {} not supported", v_codec);
                        v_match = false;
                    }

            if !support.audio_codecs.is_empty()
                && let Some(ref a_codec) = item.audio_codec
                    && !support.audio_codecs.contains(&a_codec.to_lowercase()) {
                        debug!("Audio codec {} not supported", a_codec);
                        a_match = false;
                    }

            if format_match && v_match && a_match {
                debug!(
                    "Item {} is compatible with renderer {}",
                    item.title, self.renderer_name
                );
                return true;
            }
        }

        debug!(
            "Item {} is NOT compatible with renderer {}",
            item.title, self.renderer_name
        );
        false
    }
}
