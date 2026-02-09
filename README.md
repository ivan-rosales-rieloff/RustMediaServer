# Rust DLNA Media Server

A high-performance, production-ready DLNA/UPnP Media Server written in Rust. Stream your local media files to smart TVs, gaming consoles, and other DLNA-compatible devices on your network.

## Features

- 🚀 **High Performance** - Built with Tokio async runtime and Axum web framework
- 📺 **Wide Device Support** - Compatible with LG WebOS, Samsung, Roku, Xbox, and more
- 🔄 **On-the-fly Transcoding** - Automatic FFmpeg transcoding to MPEG-TS for incompatible formats
- 📁 **Live File Watching** - Automatic library updates when files are added/removed
- ⚙️ **Renderer Profiles** - Device-specific configurations for optimal compatibility
- 🔍 **SSDP Discovery** - Automatic device detection and announcement

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           RUST DLNA MEDIA SERVER                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐  │
│  │   main.rs   │    │  state.rs   │    │  error.rs   │    │ logging.rs  │  │
│  │  (Entry)    │───▶│  (Shared)   │    │  (Errors)   │    │  (Tracing)  │  │
│  └──────┬──────┘    └─────────────┘    └─────────────┘    └─────────────┘  │
│         │                                                                   │
│         ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         HTTP LAYER (Axum)                           │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │   │
│  │  │/description │  │  /stream/   │  │ /services/  │  │ /icons/    │  │   │
│  │  │    .xml     │  │   {id}      │  │   {name}/   │  │            │  │   │
│  │  └─────────────┘  └──────┬──────┘  └──────┬──────┘  └────────────┘  │   │
│  └───────────────────────────┼───────────────┼─────────────────────────┘   │
│                              │               │                              │
│         ┌────────────────────┘               └────────────────────┐        │
│         ▼                                                         ▼        │
│  ┌─────────────────┐                                    ┌─────────────────┐│
│  │   TRANSCODING   │                                    │      UPNP       ││
│  │  ┌───────────┐  │                                    │  ┌───────────┐  ││
│  │  │transcoder │  │    ┌─────────────────────────┐    │  │  soap.rs  │  ││
│  │  │   .rs     │──┼───▶│       FFmpeg            │    │  │ (SOAP/XML)│  ││
│  │  └───────────┘  │    │  H.264 + AAC → MPEG-TS  │    │  └───────────┘  ││
│  └─────────────────┘    └─────────────────────────┘    │  ┌───────────┐  ││
│                                                         │  │dlna_helper│  ││
│                                                         │  │   .rs     │  ││
│                                                         │  └───────────┘  ││
│                                                         └─────────────────┘│
│                                                                             │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────────────┐ │
│  │    DISCOVERY    │    │     LIBRARY     │    │     CONFIGURATION       │ │
│  │  ┌───────────┐  │    │  ┌───────────┐  │    │  ┌─────────────────┐    │ │
│  │  │  ssdp.rs  │  │    │  │ scanner.rs│  │    │  │  renderer.rs    │    │ │
│  │  │(Multicast)│  │    │  │           │  │    │  │ (Device Configs)│    │ │
│  │  └───────────┘  │    │  └───────────┘  │    │  └─────────────────┘    │ │
│  │  ┌───────────┐  │    │  ┌───────────┐  │    │  ┌─────────────────┐    │ │
│  │  │registry.rs│  │    │  │media_item │  │    │  │  LG-WebOS.conf  │    │ │
│  │  │ (Devices) │  │    │  │   .rs     │  │    │  │  Samsung.conf   │    │ │
│  │  └───────────┘  │    │  └───────────┘  │    │  │  Roku-TV.conf   │    │ │
│  └─────────────────┘    └─────────────────┘    └─────────────────────────┘ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
        ┌─────────────────────────────────────────────────────────────┐
        │                     NETWORK (UDP + TCP)                     │
        │  ┌─────────────────┐              ┌─────────────────────┐   │
        │  │  SSDP Multicast │              │    HTTP Streaming   │   │
        │  │ 239.255.255.250 │              │     Port 3000       │   │
        │  │    Port 1900    │              │                     │   │
        │  └─────────────────┘              └─────────────────────┘   │
        └─────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
        ┌─────────────────────────────────────────────────────────────┐
        │                      DLNA CLIENTS                           │
        │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
        │  │ LG WebOS │  │ Samsung  │  │   Roku   │  │   Xbox   │    │
        │  │    TV    │  │    TV    │  │    TV    │  │          │    │
        │  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
        └─────────────────────────────────────────────────────────────┘
```

## DLNA Protocol Flow

```
    DLNA Client                    Rust Media Server
         │                                │
         │  ◄──── SSDP M-SEARCH ─────────│  (Discovery)
         │                                │
         │────── SSDP Response ──────────▶│
         │                                │
         │────── GET /description.xml ───▶│  (Device Info)
         │◄───── XML Device Description ──│
         │                                │
         │────── SOAP GetProtocolInfo ───▶│  (Capabilities)
         │◄───── Supported Formats ───────│
         │                                │
         │────── SOAP Browse (Root) ─────▶│  (Content List)
         │◄───── DIDL-Lite XML ───────────│
         │                                │
         │────── GET /stream/{id} ───────▶│  (Playback)
         │◄───── Video Stream ────────────│
         │       (Direct or Transcoded)   │
         │                                │
```

## Project Structure

```
rust_dlna_server/
├── Cargo.toml                 # Dependencies and metadata
├── src/
│   ├── main.rs                # Entry point, HTTP routes, stream handler
│   ├── state.rs               # Shared application state (library, renderers)
│   ├── error.rs               # Custom error types (AppError)
│   ├── logging.rs             # Tracing/logging configuration
│   ├── configuration/
│   │   ├── mod.rs
│   │   └── renderer.rs        # Renderer config parser (.conf files)
│   ├── discovery/
│   │   ├── mod.rs
│   │   ├── ssdp.rs            # SSDP multicast discovery service
│   │   └── registry.rs        # Device registry (discovered devices)
│   ├── library/
│   │   ├── mod.rs
│   │   ├── scanner.rs         # File system scanner
│   │   └── media_item.rs      # Media item struct
│   ├── transcoding/
│   │   ├── mod.rs
│   │   └── transcoder.rs      # FFmpeg spawner for on-the-fly transcoding
│   └── upnp/
│       ├── mod.rs
│       ├── soap.rs            # SOAP handlers (Browse, GetProtocolInfo)
│       └── dlna_helper.rs     # DLNA feature flag generator
├── config/                    # Renderer configuration files
│   ├── DefaultRenderer.conf
│   ├── LG-WebOS.conf
│   ├── Samsung-9series.conf
│   ├── Roku-TV.conf
│   └── ...
├── resources/
│   └── description.xml        # UPnP device description template
└── media/                     # Default media directory
```

## Implementation Details

### SSDP Discovery (`discovery/ssdp.rs`)

- Binds to UDP multicast group `239.255.255.250:1900`
- Sends periodic `NOTIFY` announcements for:
  - `upnp:rootdevice`
  - `urn:schemas-upnp-org:device:MediaServer:1`
  - `urn:schemas-upnp-org:service:ContentDirectory:1`
  - `urn:schemas-upnp-org:service:ConnectionManager:1`
- Responds to `M-SEARCH` requests from clients
- Discovers and registers other UPnP devices on the network

### SOAP Handlers (`upnp/soap.rs`)

| Endpoint | Action | Description |
|----------|--------|-------------|
| `/services/ContentDirectory/control` | `Browse` | Returns DIDL-Lite XML with media items |
| `/services/ConnectionManager/control` | `GetProtocolInfo` | Returns supported MIME types and DLNA flags |
| `/services/X_MS_MediaReceiverRegistrar/control` | `IsAuthorized` | Always returns authorized (for Xbox/Windows) |

### DLNA Feature Flags (`upnp/dlna_helper.rs`)

Generates proper DLNA.ORG headers based on content and renderer:

```
DLNA.ORG_PN=*                 # Profile name (wildcard for transcoded)
DLNA.ORG_OP=10                # Seek operations (10=time seek)
DLNA.ORG_CI=1                 # Conversion indicator (1=transcoded)
DLNA.ORG_FLAGS=01700000...    # Feature flags
```

### Transcoding (`transcoding/transcoder.rs`)

FFmpeg command for incompatible formats:
```
ffmpeg -i <input> \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -c:a aac -b:a 192k \
  -pix_fmt yuv420p \
  -f mpegts \
  -maxrate 50M -bufsize 100M \
  -
```

### Renderer Detection

1. **User-Agent matching** - Regex patterns from `.conf` files
2. **UPnP Details matching** - Device descriptions from SSDP
3. **IP-based lookup** - Registry of discovered devices

## Usage

### Prerequisites

- **Rust 1.75+** (edition 2024)
- **FFmpeg** in PATH (for transcoding)

### Installation

```bash
git clone https://github.com/your-repo/rust-dlna-server
cd rust-dlna-server
cargo build --release
```

### Running

```bash
# Default: serves ./media on port 3000
cargo run --release

# Custom directory and port
cargo run --release -- --content-dir /path/to/media --port 8080

# With debug logging
RUST_LOG=debug cargo run --release
```

### Command Line Options

```
rust_dlna_server [OPTIONS]

Options:
  -c, --content-dir <PATH>   Directory containing media files [default: media]
  -p, --port <PORT>          HTTP port to listen on [default: 3000]
  -h, --help                 Print help
  -V, --version              Print version
```

## Use Cases

### 1. Home Media Streaming

Stream your personal video collection to your living room TV without any additional software on the TV.

```
[NAS/PC with Videos] ──── WiFi ────▶ [Smart TV]
         │                              │
    rust_dlna_server              DLNA Client
    (serves files)               (plays videos)
```

### 2. Format Compatibility

MKV files with unsupported codecs are automatically transcoded:

```
Original: video.mkv (H.265 + DTS)
         │
         ▼
   [Transcoder]
         │
         ▼
Streamed: MPEG-TS (H.264 + AAC)
         │
         ▼
   [LG WebOS TV] ✓ Plays!
```

### 3. Multi-Device Support

Different devices receive optimized streams based on their capabilities:

| Device | Format | DLNA Profile | Transcoding |
|--------|--------|--------------|-------------|
| LG WebOS | MKV (H.264) | MKV_MP_HD | No |
| LG WebOS | MKV (H.265) | video/mpeg | Yes → MPEG-TS |
| Samsung | MP4 (H.264) | AVC_MP4 | No |
| Xbox | Most formats | * | No |

## Configuration

### Renderer Profiles

Create custom renderer profiles in `config/` directory:

```conf
# MyDevice.conf
RendererName = My Custom Device
UserAgentSearch = MyDevice/.*
SeekByTime = true

# Supported formats
Supported = f:mp4   v:h264   a:aac   m:video/mp4
Supported = f:mkv   v:h264   a:aac   m:video/x-matroska
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Log level (error, warn, info, debug, trace) | `info` |

## Testing

### Device Simulation

```powershell
# Run the test script
.\test_devices.ps1
```

Tests GetProtocolInfo and Browse endpoints with LG, Samsung, and Roku User-Agents.

## License

MIT License - See LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `cargo test` and `cargo clippy`
5. Submit a pull request
