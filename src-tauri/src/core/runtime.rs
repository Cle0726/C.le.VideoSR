use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct MediaRuntimeInfo {
    pub ffmpeg_available: bool,
    pub ffprobe_available: bool,
    pub ffmpeg_version: Option<String>,
    pub libx264: bool,
    pub libx265: bool,
}

fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn detect_media_runtime() -> MediaRuntimeInfo {
    let version_output = Command::new("ffmpeg").arg("-version").output().ok();
    let ffmpeg_available = version_output
        .as_ref()
        .map(|output| output.status.success())
        .unwrap_or(false);

    let ffmpeg_version = version_output.and_then(|output| {
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout)
            .ok()
            .and_then(|text| text.lines().next().map(str::to_owned))
    });

    let encoders = if ffmpeg_available {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .unwrap_or_default()
    } else {
        String::new()
    };

    MediaRuntimeInfo {
        ffmpeg_available,
        ffprobe_available: command_available("ffprobe"),
        ffmpeg_version,
        libx264: encoders.contains("libx264"),
        libx265: encoders.contains("libx265"),
    }
}
