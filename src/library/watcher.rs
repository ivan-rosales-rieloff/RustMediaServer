use crate::library::media_item::MediaItem;
use crate::state::SharedState;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tracing::{error, info, warn};

/// Monitors the media directory for changes and updates the library state.
pub struct LibraryWatcher {
    state: SharedState,
    path: PathBuf,
}

impl LibraryWatcher {
    /// Creates a new LibraryWatcher.
    ///
    /// # Arguments
    ///
    /// * `state` - The shared application state to update.
    /// * `path` - The root directory to watch.
    pub fn new(state: SharedState, path: PathBuf) -> Self {
        Self { state, path }
    }

    /// Starts the watcher loop.
    ///
    /// Performs an initial scan of the directory, then sets up a file system watcher
    /// to react to file creation, modification, and deletion events in real-time.
    pub async fn start(&self) {
        info!("Starting scanner for {:?}", self.path);

        // Initial scan
        self.scan_directory(&self.path).await;

        // Setup watcher
        let path = self.path.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        std::thread::spawn(move || {
            let (tx_deb, rx_deb) = std::sync::mpsc::channel();

            let mut debouncer = match new_debouncer(Duration::from_secs(2), None, tx_deb) {
                Ok(d) => d,
                Err(e) => {
                    error!("Failed to create debouncer: {}", e);
                    return;
                }
            };

            // We explicitly use the Watcher trait here
            if let Err(e) = debouncer.watch(&path, RecursiveMode::Recursive) {
                error!("Failed to watch directory: {}", e);
                return;
            }

            for result in rx_deb {
                match result {
                    Ok(events) => {
                        let _ = tx.send(events);
                    }
                    Err(e) => error!("Watch error: {:?}", e),
                }
            }
        });

        // Process events
        while let Some(events) = rx.recv().await {
            for event in events {
                // Iterate by reference to avoid move error if wrapper expects it
                for path in &event.paths {
                    if path.exists() {
                        self.process_file(path).await;
                    } else {
                        self.remove_file(path).await;
                    }
                }
            }
        }
    }

    /// Recursively scans a directory for media files.
    ///
    /// # Arguments
    ///
    /// * `path` - The directory path to scan.
    async fn scan_directory(&self, path: &Path) {
        let mut read_dir = match fs::read_dir(path).await {
            Ok(rd) => rd,
            Err(e) => {
                error!("Failed to read dir {:?}: {}", path, e);
                return;
            }
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                Box::pin(self.scan_directory(&path)).await;
            } else {
                self.process_file(&path).await;
            }
        }
    }

    /// Processes a potentially new or modified file.
    ///
    /// Checks if the file extension is supported (mp4, mkv, etc.). If so,
    /// it probes the file metadata using FFprobe and adds it to the library.
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file to process.
    async fn process_file(&self, path: &Path) {
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            if !["mp4", "mkv", "avi", "mov", "ts"].contains(&ext_str.as_str()) {
                return;
            }
        } else {
            return;
        }

        match self.probe_file(path).await {
            Ok(item) => {
                info!("Added/Updated: {}", item.title);
                self.state.write().unwrap().add_item(item);
            }
            Err(e) => warn!("Failed to probe {:?}: {}", path, e),
        }
    }

    /// Removes a file from the library.
    ///
    /// Calculates the ID (hash) of the file path and removes it from the shared state.
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file to remove.
    async fn remove_file(&self, path: &Path) {
        let id_hash = md5::compute(path.to_string_lossy().as_bytes());
        let id = format!("{:x}", id_hash);

        self.state.write().unwrap().remove_item(&id);
        info!("Removed: {:?}", path);
    }

    /// Probes file metadata using `ffprobe`.
    ///
    /// Spawns an `ffprobe` process to extract format info (duration, bitrate, etc.)
    /// and tags (title).
    ///
    /// # Arguments
    ///
    /// * `path` - The path of the file to probe.
    async fn probe_file(&self, path: &Path) -> anyhow::Result<MediaItem> {
        let output = tokio::process::Command::new("ffprobe")
            .arg("-v")
            .arg("quiet")
            .arg("-print_format")
            .arg("json")
            .arg("-show_format")
            .arg("-show_streams")
            .arg(path)
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("FFprobe failed"));
        }

        let json: Value = serde_json::from_slice(&output.stdout)?;

        // Safe access to JSON fields
        let format_obj = json
            .get("format")
            .ok_or(anyhow::anyhow!("No format info"))?;
        let title = path.file_stem().unwrap().to_string_lossy().to_string();

        let meta_title = format_obj
            .get("tags")
            .and_then(|t| t.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or(&title);

        let size = format_obj
            .get("size")
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);

        // Removed unused duration

        // Extract Format Name (Container)
        let container = format_obj
            .get("format_name")
            .and_then(|v| v.as_str())
            .map(|s| s.split(',').next().unwrap_or(s).to_string()); // Take first if comma separated

        // Extract Streams
        let streams = json.get("streams").and_then(|v| v.as_array());

        let mut video_codec = None;
        let mut audio_codec = None;

        if let Some(streams) = streams {
            for stream in streams {
                let codec_type = stream.get("codec_type").and_then(|v| v.as_str());
                let codec_name = stream
                    .get("codec_name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if codec_type == Some("video") && video_codec.is_none() {
                    video_codec = codec_name;
                } else if codec_type == Some("audio") && audio_codec.is_none() {
                    audio_codec = codec_name;
                }
            }
        }

        let id_hash = md5::compute(path.to_string_lossy().as_bytes());
        let id = format!("{:x}", id_hash);

        Ok(MediaItem {
            id,
            path: path.to_path_buf(),
            title: meta_title.to_string(),
            mime_type: "video/mp4".to_string(), // TODO: Infer correct mime from extensions/format
            size,
            container,
            video_codec,
            audio_codec,
        })
    }
}
