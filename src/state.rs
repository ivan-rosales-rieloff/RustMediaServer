use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub use crate::library::media_item::MediaItem;

// Removed localized MediaItem definition

use crate::configuration::renderer::RendererConfiguration;

use crate::discovery::registry::DeviceRegistry;

/// The global application state, protected by a lock in SharedState.
#[derive(Debug)]
pub struct AppState {
    /// In-memory index of media items, keyed by ID (or path hash).
    pub library: HashMap<String, MediaItem>,
    /// Incremented whenever the library changes (for CDS updates).
    pub system_update_id: u32,
    /// Persistent UUID for the server (SSDP).
    pub uuid: String,
    /// Directory being watched/served.
    #[allow(dead_code)] // Stored for potential future use (e.g., re-scan, configuration)
    pub content_dir: PathBuf,
    /// Loaded renderer configurations
    pub renderers: Vec<RendererConfiguration>,
    /// Default renderer configuration
    pub default_renderer: RendererConfiguration,
    /// Discovered UPnP/DLNA devices
    pub registry: DeviceRegistry,
}

impl AppState {
    /// Creates a new AppState.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The permanent unique identifier for this server instance.
    /// * `content_dir` - The path to the directory serving media files.
    pub fn new(
        uuid: String,
        content_dir: PathBuf,
        renderers: Vec<RendererConfiguration>,
        default_renderer: RendererConfiguration,
    ) -> Self {
        Self {
            library: HashMap::new(),
            system_update_id: 1, // Start at 1
            uuid,
            content_dir,
            renderers,
            default_renderer,
            registry: DeviceRegistry::new(),
        }
    }

    /// Adds or updates a media item in the library.
    ///
    /// Increments the `system_update_id` to signal changes to UPnP clients.
    pub fn add_item(&mut self, item: MediaItem) {
        self.library.insert(item.id.clone(), item);
        self.system_update_id = self.system_update_id.wrapping_add(1);
    }

    /// Removes a media item from the library by ID.
    ///
    /// Increments the `system_update_id` if the item existed.
    pub fn remove_item(&mut self, id: &str) {
        if self.library.remove(id).is_some() {
            self.system_update_id = self.system_update_id.wrapping_add(1);
        }
    }
}

/// Thread-safe standard wrapper.
/// Using std::sync::RwLock as requested for potential sync with notify thread,
/// but tokio::sync::RwLock is also viable.
/// Specs said: "Use tokio::sync::RwLock or std::sync::RwLock appropriately".
/// Since we might share this with async handlers and blocking watcher,
/// std::sync::RwLock is often easier for mixed sync/async if contention is low,
/// but tokio::sync::RwLock is better for holding across await.
/// given "Safety: Use ... appropriately to prevent deadlocks", and we have async handlers...
/// Let's stick to std::sync::RwLock for simple state reads in handlers (no awaits inside lock)
/// to avoid blocking async runtime for long.
/// But watcher is blocking.
/// Let's use `std::sync::RwLock` for now as it's simpler for shared state that doesn't need await inside.
pub type SharedState = Arc<RwLock<AppState>>;
