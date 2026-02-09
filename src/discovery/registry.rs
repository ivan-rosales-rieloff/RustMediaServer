use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crate::configuration::renderer::RendererConfiguration;

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields stored for logging/debugging and future use
pub struct UpnpDetails {
    pub friendly_name: String,
    pub manufacturer: String,
    pub model_name: String,
    pub model_number: String,
    pub model_description: String,
    pub manufacturer_url: String,
    pub model_url: String,
    pub address: String,
    pub udn: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields stored for registry management and future use
pub struct RegisteredDevice {
    pub ip: IpAddr,
    pub renderer: RendererConfiguration,
    pub details: UpnpDetails,
    pub last_seen: SystemTime,
}

#[derive(Debug, Clone)]
pub struct DeviceRegistry {
    devices: Arc<RwLock<HashMap<IpAddr, RegisteredDevice>>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            devices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, ip: IpAddr, device: RegisteredDevice) {
        if let Ok(mut map) = self.devices.write() {
            map.insert(ip, device);
        }
    }

    pub fn get_renderer(&self, ip: &IpAddr) -> Option<RendererConfiguration> {
        if let Ok(map) = self.devices.read()
            && let Some(device) = map.get(ip) {
                return Some(device.renderer.clone());
            }
        None
    }
}
