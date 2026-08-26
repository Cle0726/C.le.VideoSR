use std::{path::Path, process::Command};

pub fn build_ai_mux_command(
    video_only: &Path,
    source: &Path,
    output: &Path,
    duration_seconds: f64,
) -> Result<Command, String> {
    if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
        return Err("Final mux requires a known positive video duration. / 最终封装需要有效的视频时长。".into());
    }

    let extension = output
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "Output must be MP4 or MKV. / 输出文件必须为 MP4 或 MKV。".to_string())?;
    let is_mkv = extension == "mkv";
    let is_mp4 = extension == "mp4";
    if !is_mkv && !is_mp4 {
        return Err("Output must be MP4 or MKV. / 输出文件必须为 MP4 或 MKV。".into());
    }

    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(video_only)
        .arg("-i")
        .arg(source)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a?");

    // MKV can safely carry source subtitle codecs and attachments without conversion.
    // MP4 deliberately omits them because image-based/source-specific subtitle codecs
    // cannot always be converted to mov_text without failing the whole AI job.
    if is_mkv {
        command
            .arg("-map")
            .arg("1:s?")
            .arg("-map")
            .arg("1:t?");
    }

    command
        .arg("-map_metadata")
        .arg("1")
        .arg("-map_chapters")
        .arg("1")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k");

    if is_mkv {
        command.arg("-c:s").arg("copy").arg("-c:t").arg("copy");
    }

    // Make the enhanced video timeline authoritative. This prevents a short audio
    // track from truncating the output while still clipping overlong audio to the
    // source/video duration.
    command.arg("-t").arg(format!("{duration_seconds:.6}"));

    if is_mp4 {
        command.arg("-movflags").arg("+faststart");
    }

    command.arg(output);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::build_ai_mux_command;
    use std::{path::Path, process::Command};

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect()
    }

    fn has_pair(args: &[String], first: &str, second: &str) -> bool {
        args.windows(2)
            .any(|window| window[0] == first && window[1] == second)
    }

    #[test]
    fn mkv_preserves_chapters_subtitles_and_attachments_without_shortest() {
        let command = build_ai_mux_command(
            Path::new("video-only.mkv"),
            Path::new("source.mkv"),
            Path::new("output.mkv"),
            15.25,
        )
        .unwrap();
        let args = args(&command);

        assert!(!args.iter().any(|arg| arg == "-shortest"));
        assert!(has_pair(&args, "-map_chapters", "1"));
        assert!(has_pair(&args, "-map", "1:s?"));
        assert!(has_pair(&args, "-map", "1:t?"));
        assert!(has_pair(&args, "-c:s", "copy"));
        assert!(has_pair(&args, "-c:t", "copy"));
        assert!(has_pair(&args, "-t", "15.250000"));
    }

    #[test]
    fn mp4_uses_faststart_and_avoids_unsafe_subtitle_copy() {
        let command = build_ai_mux_command(
            Path::new("video-only.mkv"),
            Path::new("source.mkv"),
            Path::new("output.mp4"),
            20.0,
        )
        .unwrap();
        let args = args(&command);

        assert!(has_pair(&args, "-movflags", "+faststart"));
        assert!(!has_pair(&args, "-map", "1:s?"));
        assert!(!has_pair(&args, "-map", "1:t?"));
        assert!(has_pair(&args, "-map_chapters", "1"));
        assert!(has_pair(&args, "-t", "20.000000"));
    }

    #[test]
    fn rejects_invalid_duration_and_container() {
        assert!(build_ai_mux_command(
            Path::new("video.mkv"),
            Path::new("source.mkv"),
            Path::new("output.mkv"),
            0.0,
        )
        .is_err());
        assert!(build_ai_mux_command(
            Path::new("video.mkv"),
            Path::new("source.mkv"),
            Path::new("output.avi"),
            10.0,
        )
        .is_err());
    }
}
