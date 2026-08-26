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
    pub nominal_frame_rate: Option<f64>,
    pub frame_count: Option<u64>,
    pub variable_frame_rate: bool,
    pub video_codec: Option<String>,
    pub pixel_format: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub high_bit_depth: bool,
    pub hdr: bool,
    pub interlaced: bool,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub color_space: Option<String>,
    pub audio_codec: Option<String>,
    pub subtitle_streams: usize,
    pub attachment_streams: usize,
    pub container: Option<String>,
}

pub fn probe_media(path: &Path) -> Result<MediaProbe, String> {
    if !path.exists() {
        return Err("selected media file does not exist / 选择的媒体文件不存在".into());
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
                "ffprobe was not found. Add FFmpeg to the managed runtime or PATH. / 未找到 ffprobe，请配置 FFmpeg 运行时或 PATH。".to_string()
            } else {
                format!("failed to launch ffprobe: {error} / 启动 ffprobe 失败：{error}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "ffprobe failed to inspect the selected file / ffprobe 无法解析所选文件".into()
        } else {
            stderr
        });
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid ffprobe response: {error} / ffprobe 返回数据无效：{error}"))?;

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
    let subtitle_streams = streams
        .iter()
        .filter(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("subtitle"))
        .count();
    let attachment_streams = streams
        .iter()
        .filter(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("attachment"))
        .count();

    let format = value.get("format");
    let frame_rate = video
        .and_then(|stream| stream.get("avg_frame_rate"))
        .and_then(Value::as_str)
        .and_then(parse_ratio);
    let nominal_frame_rate = video
        .and_then(|stream| stream.get("r_frame_rate"))
        .and_then(Value::as_str)
        .and_then(parse_ratio);
    let variable_frame_rate = likely_variable_frame_rate(frame_rate, nominal_frame_rate);
    let pixel_format = video
        .and_then(|stream| stream.get("pix_fmt"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let bits_per_raw_sample = video
        .and_then(|stream| stream.get("bits_per_raw_sample"))
        .and_then(parse_optional_u32);
    let high_bit_depth = bits_per_raw_sample.is_some_and(|bits| bits > 8)
        || pixel_format.as_deref().is_some_and(pixel_format_is_high_bit_depth);
    let color_transfer = video
        .and_then(|stream| stream.get("color_transfer"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let hdr = color_transfer
        .as_deref()
        .is_some_and(|value| matches!(value, "smpte2084" | "arib-std-b67"));
    let interlaced = video
        .and_then(|stream| stream.get("field_order"))
        .and_then(Value::as_str)
        .is_some_and(|value| !matches!(value, "progressive" | "unknown"));

    Ok(MediaProbe {
        path: path.to_string_lossy().into_owned(),
        duration_seconds: format
            .and_then(|item| item.get("duration"))
            .and_then(Value::as_str)
            .and_then(|duration| duration.parse::<f64>().ok()),
        width: video
            .and_then(|stream| stream.get("width"))
            .and_then(Value::as_u64),
        height: video
            .and_then(|stream| stream.get("height"))
            .and_then(Value::as_u64),
        frame_rate,
        nominal_frame_rate,
        frame_count: video
            .and_then(|stream| stream.get("nb_frames"))
            .and_then(parse_optional_u64),
        variable_frame_rate,
        video_codec: video
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        pixel_format,
        bits_per_raw_sample,
        high_bit_depth,
        hdr,
        interlaced,
        color_primaries: video
            .and_then(|stream| stream.get("color_primaries"))
            .and_then(Value::as_str)
            .map(str::to_string),
        color_transfer,
        color_space: video
            .and_then(|stream| stream.get("color_space"))
            .and_then(Value::as_str)
            .map(str::to_string),
        audio_codec: audio
            .and_then(|stream| stream.get("codec_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        subtitle_streams,
        attachment_streams,
        container: format
            .and_then(|item| item.get("format_name"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub fn validate_ai_media(path: &Path) -> Result<MediaProbe, String> {
    let probe = probe_media(path)?;
    if probe.variable_frame_rate {
        return Err("Variable-frame-rate video is not yet timestamp-safe in the AI pipeline. Convert to CFR first or wait for the timestamp-preserving pipeline. / 当前 AI 管线还不能安全保留可变帧率时间戳，请先转为固定帧率或等待时间戳保留管线。".into());
    }
    if probe.hdr || probe.high_bit_depth {
        return Err("HDR / >8-bit video is blocked to prevent silent color-depth loss in the current 8-bit AI encoder path. / 为避免当前 8-bit AI 编码链静默损失 HDR 或高位深信息，已暂时阻止此类视频。".into());
    }
    if probe.interlaced {
        return Err("Interlaced video must be deinterlaced before AI enhancement. / 隔行扫描视频需要先去交错再进行 AI 增强。".into());
    }
    Ok(probe)
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

fn parse_optional_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        .filter(|number| *number > 0)
}

fn parse_optional_u32(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .or_else(|| value.as_str().and_then(|text| text.parse::<u32>().ok()))
        .filter(|number| *number > 0)
}

fn likely_variable_frame_rate(average: Option<f64>, nominal: Option<f64>) -> bool {
    let (Some(average), Some(nominal)) = (average, nominal) else {
        return false;
    };
    if !average.is_finite() || !nominal.is_finite() || average <= 0.0 || nominal <= 0.0 {
        return false;
    }
    let tolerance = (nominal.abs() * 0.001).max(0.01);
    (average - nominal).abs() > tolerance
}

fn pixel_format_is_high_bit_depth(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    ["p9", "p10", "p12", "p14", "p16", "9le", "9be", "10le", "10be", "12le", "12be", "14le", "14be", "16le", "16be"]
        .iter()
        .any(|marker| value.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::{likely_variable_frame_rate, parse_ratio, pixel_format_is_high_bit_depth};

    #[test]
    fn parses_fractional_frame_rates() {
        let fps = parse_ratio("24000/1001").unwrap();
        assert!((fps - 23.976).abs() < 0.001);
        assert_eq!(parse_ratio("1/0"), None);
    }

    #[test]
    fn detects_likely_vfr_from_rate_mismatch() {
        assert!(!likely_variable_frame_rate(Some(23.976), Some(23.976)));
        assert!(likely_variable_frame_rate(Some(29.7), Some(30.0)));
    }

    #[test]
    fn identifies_common_high_bit_depth_pixel_formats() {
        assert!(pixel_format_is_high_bit_depth("yuv420p10le"));
        assert!(pixel_format_is_high_bit_depth("p010le"));
        assert!(!pixel_format_is_high_bit_depth("yuv420p"));
    }
}
