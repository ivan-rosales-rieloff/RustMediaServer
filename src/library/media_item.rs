use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents a single media file in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub mime_type: String, // e.g., "video/mp4"
    pub size: u64,
    pub container: Option<String>,   // e.g., "mp4", "mkv"
    pub video_codec: Option<String>, // e.g., "h264", "hevc"
    pub audio_codec: Option<String>, // e.g., "aac", "ac3"
}

impl MediaItem {
    pub fn new(id: String, path: PathBuf, title: String, mime_type: String, size: u64) -> Self {
        Self {
            id,
            path,
            title,
            mime_type,
            size,
            container: None,
            video_codec: None,
            audio_codec: None,
        }
    }
}
