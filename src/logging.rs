use chrono::Local;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{error, info};

/// Spawns a background task that periodically scans the log directory
/// and compresses log files from previous days.
pub fn spawn_log_cleanup_task(log_dir: PathBuf) {
    tokio::spawn(async move {
        // Run immediately on startup
        cleanup_logs(&log_dir);

        // Then check every hour
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            cleanup_logs(&log_dir);
        }
    });
}

fn cleanup_logs(log_dir: &Path) {
    info!("Starting log cleanup/compression check in {:?}", log_dir);

    // Ensure log directory exists (it should, but safety first)
    if !log_dir.exists() {
        return;
    }

    let read_dir = match fs::read_dir(log_dir) {
        Ok(dir) => dir,
        Err(e) => {
            error!("Failed to read log directory: {}", e);
            return;
        }
    };

    let today = Local::now().date_naive();
    let prefix = "server.log.";

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        // Check if it matches our pattern "server.log.YYYY-MM-DD"
        // and does NOT end in .gz
        if file_name.starts_with(prefix) && !file_name.ends_with(".gz") {
            // Extract date part
            let date_str = &file_name[prefix.len()..];

            // Try to parse date
            if let Ok(file_date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                // If strictly older than today, compress it
                if file_date < today {
                    info!("Compressing old log file: {}", file_name);
                    if let Err(e) = compress_file(&path) {
                        error!("Failed to compress {}: {}", file_name, e);
                    }
                }
            }
        }
    }
}

fn compress_file(input_path: &Path) -> std::io::Result<()> {
    // Actually input is server.log.2023-10-01. with_extension replaces extension?
    // Path::new("server.log.2023-10-01").with_extension("gz") -> "server.log.2023-10-01.gz" if no extension or "server.log.gz" if it thinks date is extension?
    // "2023-10-01" is treated as extension if it follows dot?
    // Let's explicitly append .gz to be safe.
    let output_path_str = format!("{}.gz", input_path.to_string_lossy());
    let output_path = PathBuf::from(output_path_str);

    let input_file = fs::File::open(input_path)?;
    let output_file = fs::File::create(&output_path)?;

    let mut encoder = GzEncoder::new(output_file, Compression::default());
    let mut input_reader = std::io::BufReader::new(input_file);

    std::io::copy(&mut input_reader, &mut encoder)?;
    encoder.finish()?;

    // If successful, delete original
    fs::remove_file(input_path)?;

    info!("Compressed and removed: {:?}", input_path);

    Ok(())
}
