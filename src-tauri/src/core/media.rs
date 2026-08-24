use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct MediaProbe {
    pub path: String,
    pub duration_seconds: Option<f64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub frame_rate: Option<f64>,
    pub video_codec: Option<String>,
    pub pixel_format: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
}

pub fn probe_media(path: &Path) -> Result<MediaProbe, String> {
    if !path.exists() {
        return Err("selected media file does not exist".into());
    }

    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "ffprobe was not found. Install FFmpeg or add ffprobe to PATH.".to_string()
            } else {
                format!("failed to launch ffprobe: {error}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "ffprobe failed to inspect the selected file".into()
        } else {
            stderr
        });
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid ffprobe response: {error}"))?;

    let streams = value
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"));
    let audio = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"));

    let format = value.get("format");

    Ok(MediaProbe {
        path: path.to_string_lossy().into_owned(),
        duration_seconds: format
            .and_then(|item| item.get("duration"))
            .and_then(Value::as_str)
            .and_then(|duration| duration.parse::<f64>().ok()),
        width: video.and_then(|stream| stream.get("width")).and_then(Value::as_u64),
        height: video.and_then(|stream| stream.get("height")).and_then(Value::as_u64),
        frame_rate: video
            .and_then(|stream| stream.get("avg_frame_rate"))
            .and_then(Value::as_str)
            .and_then(parse_ratio),
        video_codec: video
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        pixel_format: video
            .and_then(|stream| stream.get("pix_fmt"))
            .and_then(Value::as_str)
            .map(str::to_string),
        audio_codec: audio
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        container: format
            .and_then(|item| item.get("format_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn parse_ratio(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;

    if denominator == 0.0 {
        None
    } else {
        Some(numerator / denominator)
    }
}
