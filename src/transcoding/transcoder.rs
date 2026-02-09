use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio_util::io::ReaderStream;
use tracing::info;

/// Helper struct for managing on-the-fly transcoding.
pub struct Transcoder;

impl Transcoder {
    /// Spawns an FFmpeg process to transcode a media file to a DLNA-compatible stream (MPEG-TS).
    ///
    /// It converts video to H.264 (libx264, ultrafast preset) and audio to AAC.
    /// The output is piped to standard output, which is wrapped in a `ReaderStream`
    /// for Axum to serve as a chunked HTTP response.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the source media file.
    /// * `start_position_seconds` - The seek position in seconds (for resuming).
    pub fn spawn_stream(
        file_path: &Path,
        start_position_seconds: u64,
    ) -> std::io::Result<ReaderStream<tokio::io::BufReader<tokio::process::ChildStdout>>> {
        let file_str = file_path
            .to_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Invalid path"))?;

        let mut cmd = Command::new("ffmpeg");

        // Optimize for fast startup
        cmd.arg("-analyzeduration").arg("0");
        cmd.arg("-probesize").arg("2000000"); // 2MB probe

        // Seek if needed (must be before input for fast seek)
        if start_position_seconds > 0 {
            cmd.arg("-ss").arg(start_position_seconds.to_string());
        }

        cmd.arg("-i")
            .arg(file_str)
            // Video codec
            .arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("ultrafast") // Low latency
            .arg("-tune")
            .arg("zerolatency")
            // Audio codec
            .arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("192k")
            .arg("-pix_fmt")
            .arg("yuv420p") // Ensure compatibility
            // Output format (MPEG-TS is standard for DLNA)
            .arg("-f")
            .arg("mpegts")
            // Allow bursting for faster buffering
            .arg("-maxrate")
            .arg("50M")
            .arg("-bufsize")
            .arg("100M")
            // Pipe to stdout
            .arg("-")
            .stdout(Stdio::piped())
            .stderr(Stdio::null()); // Log stderr? Maybe too noisy.

        info!("Spawning ffmpeg for {}", file_str);

        let mut child = cmd.spawn()?;

        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Failed to open ffmpeg stdout",
            )
        })?;

        // Buffer the output to reduce small syscalls and improve stream smoothness
        // 128KB buffer (Video frames can be large)
        let buffered_stdout = tokio::io::BufReader::with_capacity(128 * 1024, stdout);

        Ok(ReaderStream::new(buffered_stdout))
    }
}
