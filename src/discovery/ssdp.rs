use crate::discovery::registry::{RegisteredDevice, UpnpDetails};
use crate::state::SharedState;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::Notify;
use tracing::{error, info, trace};

const MULTICAST_IP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const MULTICAST_PORT: u16 = 1900;

/// Service for handling SSDP (Simple Service Discovery Protocol) operations.
///
/// This struct manages the UDP socket and tasks for announcing the device
/// presence and responding to discovery requests from other devices.
pub struct SSDPService {
    uuid: String,
    http_port: u16,
    #[allow(dead_code)] // Reserved for future graceful shutdown implementation
    shutdown_signal: Arc<Notify>,
    state: SharedState,
}

impl SSDPService {
    /// Creates a new instance of the SSDP service.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The unique identifier of the device.
    /// * `http_port` - The TCP port where the HTTP server is listening.
    /// * `state` - The shared application state for device registry access.
    pub fn new(uuid: String, http_port: u16, state: SharedState) -> Self {
        Self {
            uuid,
            http_port,
            shutdown_signal: Arc::new(Notify::new()),
            state,
        }
    }

    /// Starts the SSDP service.
    ///
    /// This function spawns two background tasks:
    /// 1. A listener task that handles incoming M-SEARCH requests.
    /// 2. An announcer task that periodically broadcasts NOTIFY (ssdp:alive) messages.
    ///
    /// It attempts to resolve the local IP address to bind the multicast interface correctly.
    pub async fn spawn(&self) {
        // Resolve local IP once at startup for binding
        let local_ip = match local_ip_address::local_ip() {
            Ok(std::net::IpAddr::V4(ip)) => ip,
            Ok(std::net::IpAddr::V6(_)) => {
                error!("IPv6 not supported for SSDP yet, falling back to 0.0.0.0");
                Ipv4Addr::UNSPECIFIED
            }
            Err(e) => {
                error!("Failed to get local IP: {}, falling back to 0.0.0.0", e);
                Ipv4Addr::UNSPECIFIED
            }
        };

        info!("SSDP: Detected local IP as: {}", local_ip);

        let socket = match self.create_socket(local_ip) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create SSDP socket: {}", e);
                return;
            }
        };

        let uuid = self.uuid.clone();
        let http_port = self.http_port;

        info!("SSDP Service started on port 1900");

        // Spawn Listener Task
        let socket_recv = socket.clone();
        let uuid_recv = uuid.clone();
        let state_recv = self.state.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                match socket_recv.recv_from(&mut buf).await {
                    Ok((size, peer)) => {
                        let msg = String::from_utf8_lossy(&buf[..size]);
                        if msg.starts_with("M-SEARCH") {
                            info!("SSDP: Received M-SEARCH from {}", peer);
                            Self::handle_msearch(
                                &socket_recv,
                                peer,
                                &msg,
                                &uuid_recv,
                                http_port,
                                local_ip,
                            )
                            .await;
                        } else if msg.starts_with("NOTIFY") || msg.starts_with("HTTP/1.1 200 OK") {
                            Self::handle_discovery_packet(&msg, peer, &state_recv).await;
                        }
                    }
                    Err(e) => error!("SSDP recv error: {}", e),
                }
            }
        });

        // Spawn Announcer Task (NOTIFY)
        let socket_announce = socket.clone();
        tokio::spawn(async move {
            let types = vec![
                "upnp:rootdevice",
                "urn:schemas-upnp-org:device:MediaServer:1",
                "urn:schemas-upnp-org:service:ContentDirectory:1",
                "urn:schemas-upnp-org:service:ConnectionManager:1",
                "urn:microsoft.com:service:X_MS_MediaReceiverRegistrar:1",
            ];

            let boot_id = 1; // Constant BOOTID for the session

            // Initial announcement
            for _ in 0..5 {
                // Announce rapidly at startup
                // Announce rapidly at startup
                for nt in &types {
                    Self::send_notify(&socket_announce, &uuid, http_port, nt, local_ip, boot_id)
                        .await;
                }
                let uuid_nt = format!("uuid:{}", uuid);
                Self::send_notify(
                    &socket_announce,
                    &uuid,
                    http_port,
                    &uuid_nt,
                    local_ip,
                    boot_id,
                )
                .await;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            loop {
                for nt in &types {
                    Self::send_notify(&socket_announce, &uuid, http_port, nt, local_ip, boot_id)
                        .await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }

                let uuid_nt = format!("uuid:{}", uuid);
                Self::send_notify(
                    &socket_announce,
                    &uuid,
                    http_port,
                    &uuid_nt,
                    local_ip,
                    boot_id,
                )
                .await;

                // Re-announce every 30 seconds (keep alive)
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    }

    /// Creates and configures the UDP socket for SSDP.
    ///
    /// This function sets up the socket with:
    /// * Reuse address/port options.
    /// * Multicast TTL.
    /// * Outgoing multicast interface (critical for multi-homed systems).
    /// * Multicast group membership.
    ///
    /// # Arguments
    ///
    /// * `local_ip` - The local IPv4 address to bind the multicast interface to.
    fn create_socket(&self, local_ip: Ipv4Addr) -> std::io::Result<Arc<UdpSocket>> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

        socket.set_reuse_address(true)?;
        #[cfg(not(windows))]
        socket.set_reuse_port(true)?;

        socket.set_multicast_ttl_v4(4)?;

        // Critical: Set the outgoing interface for multicast packets
        socket.set_multicast_if_v4(&local_ip)?;

        // Bind to global 0.0.0.0:1900 to receive multicasts
        // Note: On Windows, binding to a specific IP for multicast receiving can be tricky.
        // Usually binding to 0.0.0.0 is correct for receiving, but joining membership on the specific interface is key.
        socket.bind(&SocketAddr::from(([0, 0, 0, 0], MULTICAST_PORT)).into())?;

        // Critical: Join multicast group SPECIFICALLY on the interface of local_ip
        socket.join_multicast_v4(&MULTICAST_IP, &local_ip)?;
        socket.set_multicast_loop_v4(true)?;
        socket.set_nonblocking(true)?;

        let udp_socket = UdpSocket::from_std(socket.into())?;
        Ok(Arc::new(udp_socket))
    }

    /// Handles incoming SSDP M-SEARCH requests.
    ///
    /// Checks if the search target (`ST` header) matches any of the device's supported
    /// types or its UUID. If a match is found, it sends a unicast HTTP/UDP response.
    ///
    /// # Arguments
    ///
    /// * `socket` - The UDP socket to send the response from.
    /// * `peer` - The address of the client that sent the M-SEARCH.
    /// * `msg` - The raw content of the M-SEARCH packet.
    /// * `uuid` - The device UUID.
    /// * `http_port` - The HTTP server port.
    /// * `local_ip` - The local IP address (for constructing the Location URL).
    async fn handle_msearch(
        socket: &UdpSocket,
        peer: SocketAddr,
        msg: &str,
        uuid: &str,
        http_port: u16,
        local_ip: Ipv4Addr,
    ) {
        let st_header = msg
            .lines()
            .find(|l| l.to_uppercase().starts_with("ST:"))
            .map(|l| l.trim()[3..].trim());

        let target = st_header.unwrap_or("").trim();

        info!("SSDP: Check target '{}'", target);

        let known_types = vec![
            "upnp:rootdevice",
            "urn:schemas-upnp-org:device:MediaServer:1",
            "urn:schemas-upnp-org:service:ContentDirectory:1",
            "urn:schemas-upnp-org:service:ConnectionManager:1",
            "urn:microsoft.com:service:X_MS_MediaReceiverRegistrar:1",
        ];

        let my_uuid_st = format!("uuid:{}", uuid);

        if target == "ssdp:all" {
            for t in &known_types {
                Self::send_response(socket, peer, uuid, http_port, t, local_ip).await;
            }
            Self::send_response(socket, peer, uuid, http_port, &my_uuid_st, local_ip).await;
        } else if known_types.contains(&target) || target == my_uuid_st {
            Self::send_response(socket, peer, uuid, http_port, target, local_ip).await;
        }
    }

    /// Sends a unicast SSDP response to a search request.
    ///
    /// The response follows the UPnP/DLNA specification, including headers like
    /// `ST`, `USN`, `LOCATION`, `SERVER`, and `EXT`.
    ///
    /// # Arguments
    ///
    /// * `socket` - The UDP socket.
    /// * `peer` - The recipient address.
    /// * `st` - The Search Target (`ST`) to allow confirming the match.
    async fn send_response(
        socket: &UdpSocket,
        peer: SocketAddr,
        uuid: &str,
        http_port: u16,
        st: &str,
        local_ip: Ipv4Addr,
    ) {
        let location = format!("http://{}:{}/description.xml", local_ip, http_port);
        let date = chrono::Utc::now().to_rfc2822();

        let usn = if st.starts_with("uuid:") {
            st.to_string()
        } else {
            format!("uuid:{}::{}", uuid, st)
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\n\
            CACHE-CONTROL: max-age=1800\r\n\
            DATE: {}\r\n\
            EXT:\r\n\
            LOCATION: {}\r\n\
            SERVER: UPnP/1.0 DLNADOC/1.50 Rust-DLNA/1.0\r\n\
            ST: {}\r\n\
            USN: {}\r\n\
            BOOTID.UPNP.ORG: 0\r\n\
            CONFIGID.UPNP.ORG: 1\r\n\
            \r\n",
            date, location, st, usn
        );
        trace!("SSDP: Sending response: {}", response);
        if let Err(e) = socket.send_to(response.as_bytes(), peer).await {
            error!("Failed to send SSDP response: {}", e);
        } else {
            info!("SSDP: Sent matched response for {} to {}", st, peer);
        }
    }

    /// Multicasts an SSDP NOTIFY message to announce device presence.
    ///
    /// This is sent to `239.255.255.250:1900` to let all devices on the network
    /// know that this server is alive and available.
    ///
    /// # Arguments
    ///
    /// * `nt` - The Notification Type (`NT`) being announced (e.g., `upnp:rootdevice`).
    async fn send_notify(
        socket: &UdpSocket,
        uuid: &str,
        http_port: u16,
        nt: &str,
        local_ip: Ipv4Addr,
        boot_id: u32,
    ) {
        let location = format!("http://{}:{}/description.xml", local_ip, http_port);
        let dest = SocketAddrV4::new(MULTICAST_IP, MULTICAST_PORT);

        let usn = if nt.starts_with("uuid:") {
            nt.to_string()
        } else {
            format!("uuid:{}::{}", uuid, nt)
        };

        let msg = format!(
            "NOTIFY * HTTP/1.1\r\n\
            HOST: {}:{}\r\n\
            CACHE-CONTROL: max-age=1800\r\n\
            LOCATION: {}\r\n\
            NT: {}\r\n\
            NTS: ssdp:alive\r\n\
            SERVER: UPnP/1.0 DLNADOC/1.50 Rust-DLNA/1.0\r\n\
            USN: {}\r\n\
            BOOTID.UPNP.ORG: {}\r\n\
            CONFIGID.UPNP.ORG: 1\r\n\
            \r\n",
            MULTICAST_IP, MULTICAST_PORT, location, nt, usn, boot_id
        );
        trace!("SSDP: Sending notify: {}", msg);

        if let Err(e) = socket.send_to(msg.as_bytes(), dest).await {
            match e.kind() {
                std::io::ErrorKind::WouldBlock => { /* ignore */ }
                _ => error!("Failed to send SSDP Notify: {}", e),
            }
        }
    }

    async fn handle_discovery_packet(msg: &str, peer: SocketAddr, state: &SharedState) {
        let location = msg
            .lines()
            .find(|l| l.to_uppercase().starts_with("LOCATION:"))
            .map(|l| l[9..].trim());

        if let Some(loc) = location {
            // Check if we should fetch (optimization: check registry timestamp?)
            // For now, fetch every time (or let the registry debounce)
            let loc_url = loc.to_string();
            let state_clone = state.clone();
            let peer_ip = peer.ip();

            // Spawn a detached task to fetch so we don't block the UDP loop
            tokio::spawn(async move {
                if let Err(e) = Self::fetch_and_register(&loc_url, peer_ip, state_clone).await {
                    trace!("Failed to fetch/register device at {}: {}", loc_url, e);
                }
            });
        }
    }

    async fn fetch_and_register(
        location: &str,
        ip: IpAddr,
        state: SharedState,
    ) -> anyhow::Result<()> {
        let resp = reqwest::get(location).await?.text().await?;
        let doc = roxmltree::Document::parse(&resp)
            .map_err(|e| anyhow::anyhow!("XML Parse Error: {}", e))?;

        let device = doc
            .descendants()
            .find(|n| n.has_tag_name("device"))
            .ok_or_else(|| anyhow::anyhow!("No device tag found"))?;

        let get_text = |tag: &str| -> String {
            device
                .descendants()
                .find(|n| n.has_tag_name(tag))
                .and_then(|n| n.text())
                .unwrap_or("")
                .to_string()
        };

        let friendly_name = get_text("friendlyName");
        let manufacturer = get_text("manufacturer");
        let model_name = get_text("modelName");
        let model_number = get_text("modelNumber");
        let model_description = get_text("modelDescription");
        let manufacturer_url = get_text("manufacturerURL");
        let model_url = get_text("modelURL");
        let udn = get_text("UDN");

        // Extract host from location for 'address' field
        let address = reqwest::Url::parse(location)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| ip.to_string());

        // Construct UMS Details String
        // Order: friendlyName address udn manufacturer modelName modelNumber modelDescription manufacturerURL modelURL
        let details_string = format!(
            "{} {} {} {} {} {} {} {} {}",
            friendly_name,
            address,
            udn,
            manufacturer,
            model_name,
            model_number,
            model_description,
            manufacturer_url,
            model_url
        );

        trace!("Checking UMS Details: '{}'", details_string);

        // Check against Renderers
        let matched_renderer = {
            let read_state = state.read().map_err(|_| anyhow::anyhow!("Lock poison"))?;
            let mut found = None;

            // UMS Logic:
            // 1. Check strict match
            // 2. Loading priority is handled by pre-sorting renderers in main.rs
            for renderer in &read_state.renderers {
                if renderer.match_upnp_details(&details_string) {
                    info!(
                        "SSDP: Matched device at {} to renderer {}",
                        ip, renderer.renderer_name
                    );
                    found = Some(renderer.clone());
                    break;
                }
            }
            found
        };

        if let Some(renderer) = matched_renderer {
            let details = UpnpDetails {
                friendly_name,
                manufacturer,
                model_name,
                model_number,
                model_description,
                manufacturer_url,
                model_url,
                address,
                udn,
            };

            let reg_device = RegisteredDevice {
                ip,
                renderer,
                details,
                last_seen: std::time::SystemTime::now(),
            };

            let read_state = state.write().map_err(|_| anyhow::anyhow!("Lock poison"))?;
            read_state.registry.register(ip, reg_device);
        }

        Ok(())
    }
}
