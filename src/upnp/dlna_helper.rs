use crate::configuration::renderer::RendererConfiguration;

/// Returns DLNA.ORG_PN flags for a given MIME type and renderer configuration.
///
/// Respects renderer settings for `send_dlna_org_flags` and `dlna_org_pn_used`.
///
/// # Arguments
/// * `mime_type` - The MIME type of the content
/// * `renderer` - The renderer configuration
/// * `is_transcoded` - Whether the content is being transcoded (affects DLNA.ORG_CI)
pub fn get_dlna_org_pn_flags(
    mime_type: &str,
    renderer: &RendererConfiguration,
    is_transcoded: bool,
) -> String {
    // Some renderers work better without DLNA flags entirely
    if !renderer.send_dlna_org_flags {
        return "".to_string();
    }

    // DLNA.ORG_OP
    // 00 - no seek
    // 01 - range seek (bytes)
    // 10 - time seek (DLNA time-based seek)
    // 11 - both range and time seek
    let op = if is_transcoded {
        // For transcoded streams, we typically can't do byte-range seeks
        // Use time-based seek if supported, else no seek
        if renderer.seek_by_time == "true" || renderer.seek_by_time == "exclusive" {
            "10" // Time seek only
        } else {
            "00" // No seek for transcoded content without time seek support
        }
    } else if renderer.seek_by_time == "true" {
        "11" // Both range and time seek
    } else if renderer.seek_by_time == "exclusive" {
        "10" // Time seek only
    } else {
        "01" // Range seek (bytes) only
    };

    // DLNA.ORG_CI - Conversion Indicator
    // 0 = original content (not transcoded)
    // 1 = transcoded content
    let ci = if is_transcoded { "1" } else { "0" };

    // Standard DLNA flags for streaming
    // Bits: SENDER_PACED, LSOP_TIME_BASED_SEEK, LSOP_BYTE_BASED_SEEK, etc.
    let flags = "01700000000000000000000000000000";

    // If renderer doesn't want DLNA.ORG_PN, return only OP/CI/FLAGS
    if !renderer.dlna_org_pn_used {
        return format!(
            "DLNA.ORG_OP={};DLNA.ORG_CI={};DLNA.ORG_FLAGS={}",
            op, ci, flags
        );
    }

    // Determine DLNA.ORG_PN based on MIME type
    // For transcoded content, use wildcard (*) for maximum compatibility
    // Many TVs (especially LG WebOS) reject specific profiles but accept wildcards
    let mut dlna_org_pn = if is_transcoded {
        // For transcoded MPEG-TS, use wildcard for broader compatibility
        // LG WebOS specifically rejects some named profiles
        "*".to_string()
    } else {
        match mime_type {
            "video/mpeg" | "video/mp2t" | "video/vnd.dlna.mpeg-tts" => {
                // For native MPEG-TS files
                "MPEG_TS_SD_NA".to_string()
            }
            "video/mp4" => "AVC_MP4_BL_CIF15_AAC_520".to_string(),
            "video/x-matroska" | "video/mkv" => "AVC_MKV_MP_HD_AAC_MULT5".to_string(),
            "audio/mpeg" => "MP3".to_string(),
            "audio/mp4" | "audio/aac" => "AAC_ISO".to_string(),
            "audio/L16" => "LPCM".to_string(),
            _ => "*".to_string(),
        }
    };

    // Apply DLNA Profile Changes from renderer config
    // e.g. DLNAProfileChanges = AVC_TS_MP_HD_AAC_ISO=AVC_TS_NA_ISO
    if let Some(replacement) = renderer.dlna_profile_changes.get(&dlna_org_pn) {
        dlna_org_pn = replacement.clone();
    }

    // Construct the full DLNA feature string
    format!(
        "DLNA.ORG_PN={};DLNA.ORG_OP={};DLNA.ORG_CI={};DLNA.ORG_FLAGS={}",
        dlna_org_pn, op, ci, flags
    )
}
