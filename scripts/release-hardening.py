from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, content: str) -> None:
    target = ROOT / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")


def replace_required(path: str, old: str, new: str, *, count: int = 1) -> None:
    text = read(path)
    actual = text.count(old)
    if actual < count:
        raise RuntimeError(f"{path}: expected at least {count} occurrence(s), found {actual}: {old[:100]!r}")
    text = text.replace(old, new, count)
    write(path, text)


MUX_RS = r'''use std::{path::Path, process::Command};

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
'''

COPY_TS = r'''export function bi(zh: string, en: string) {
  return `${zh} / ${en}`;
}

export const ui = {
  unknown: bi("未知", "Unknown"),
  localProcessing: bi("本地处理", "Local processing"),
  selectVideo: bi("选择视频", "Select video"),
  chooseAnother: bi("选择其他视频", "Choose another video"),
  inspecting: bi("正在分析…", "Inspecting…"),
  ffprobeMissing: bi("未检测到 ffprobe，请配置托管运行时或 PATH。", "ffprobe is not available in the managed runtime or PATH."),
  srTarget: bi("超分目标", "SR target"),
  readyAi: bi("本地 AI 已就绪", "Ready for local AI processing"),
  mediaReadyAiMissing: bi("媒体管线已就绪 · AI 运行时或模型缺失", "Media pipeline ready · AI runtime or model missing"),
  enhancementMode: bi("增强模式", "Enhancement mode"),
  engineAware: bi("引擎感知", "Engine-aware"),
  enhancement: bi("增强设置", "Enhancement"),
  plannedBackend: bi("后端规划中", "Planned backend"),
  superResolution: bi("超分辨率", "Super resolution"),
  upscaleModel: bi("超分模型", "Upscale model"),
  noUpscaleProfiles: bi("没有可用的极速超分模型", "No runnable Fast-mode upscale profiles"),
  frameInterpolation: bi("视频补帧", "Frame interpolation"),
  rifeModel: bi("RIFE 模型", "RIFE model"),
  noRifeProfiles: bi("没有可用的 RIFE 模型", "No runnable RIFE profiles"),
  targetFps: bi("目标帧率", "Target FPS"),
  sceneProtection: bi("场景保护 · 阈值 0.42 · 单帧重叠", "Scene protection · 0.42 threshold · one-frame overlap"),
  videoCodec: bi("视频编码", "Video codec"),
  copyM1: bi("复制源视频流 · 仅 M1", "Copy source video stream · M1 only"),
  chooseOutput: bi("选择输出文件", "Choose output file"),
  cancelProcessing: bi("取消处理", "Cancel processing"),
  enhanceVideo: bi("视频增强", "Enhance video"),
  interpolateVideo: bi("视频补帧", "Interpolate video"),
  validatePipeline: bi("验证媒体管线", "Validate media pipeline"),
  ffmpegRequired: bi("视频处理需要 FFmpeg。", "FFmpeg is required for video processing."),
  rifeMissing: bi("RIFE 运行时不可用，补帧已禁用。", "RIFE runtime is not available, so frame interpolation is disabled."),
  aiCopyUnsupported: bi("AI 会生成新视频帧，请选择 H.264 或 H.265，不能使用流复制。", "AI transforms create new video frames, so H.264 or H.265 must be selected instead of stream copy."),
  plannedModes: bi("Quality 与 AI Restore 暂时禁用，等待对应运行时接入。", "Quality and AI Restore remain disabled until their runtimes land."),
  fastPipeline: bi("极速模式使用有界帧分块，并为每个任务保持单一持久编码器。", "Fast mode uses bounded frame chunks and one persistent encoder per job."),
  source: bi("源媒体", "Source"),
  video: bi("视频", "Video"),
  audio: bi("音频", "Audio"),
  container: bi("容器", "Container"),
  frameTiming: bi("帧时间轴", "Frame timing"),
  colorDepth: bi("色彩 / 位深", "Color / bit depth"),
  auxiliaryStreams: bi("字幕 / 附件", "Subtitles / attachments"),
  mediaRuntime: bi("媒体运行时", "Media runtime"),
  ready: bi("就绪", "Ready"),
  unavailable: bi("不可用", "Unavailable"),
  detected: bi("已检测", "Detected"),
  missing: bi("缺失", "Missing"),
  aiRuntime: bi("AI 运行时", "AI runtime"),
  models: bi("模型", "Models"),
  catalog: bi("模型目录", "Catalog"),
  managedDirectory: bi("托管模型目录", "Managed directory"),
  modelNotStaged: bi("未部署模型载荷", "Model payload not staged"),
  loading: bi("加载中", "Loading"),
  system: bi("系统", "System"),
  checking: bi("检测中", "Checking"),
  platform: bi("平台", "Platform"),
  memory: bi("内存", "Memory"),
  gpuAuto: bi("NCNN 将自动选择 Vulkan GPU", "NCNN will select a Vulkan GPU automatically"),
  readingCapabilities: bi("正在读取本机能力…", "Reading local capabilities…"),
  processed: bi("已处理", "processed"),
  vfrBlocked: bi("检测到可变帧率（VFR）。当前 AI 管线为避免时间戳漂移会阻止处理。", "Variable frame rate detected. AI processing is blocked until timestamp-preserving VFR support lands."),
  hdrBlocked: bi("检测到 HDR 或高位深视频。当前 8-bit AI 编码链会造成质量损失，因此已阻止处理。", "HDR or high-bit-depth video detected. The current 8-bit AI encoder path is blocked to prevent silent quality loss."),
  interlacedBlocked: bi("检测到隔行扫描视频，请先去交错。", "Interlaced video detected; deinterlace it before AI processing."),
  mkvSubtitleHint: bi("源文件包含字幕或附件。选择 MKV 输出可无损复制这些流与章节。", "The source contains subtitles or attachments. Choose MKV to copy those streams and chapters without conversion."),
  mp4SubtitleWarning: bi("MP4 输出会保留章节，但当前不会复制源字幕/附件；如需保留请改用 MKV。", "MP4 keeps chapters but currently omits source subtitles/attachments; choose MKV to preserve them."),
  inactiveJob: bi("处理任务已不再运行。", "The processing job is no longer active."),
  cancelling: bi("正在取消…", "Cancelling…"),
};
'''

write("src-tauri/src/core/mux.rs", MUX_RS)
write("src/copy.ts", COPY_TS)

# Wire the shared mux module.
replace_required(
    "src-tauri/src/core/mod.rs",
    "pub mod models;\n",
    "pub mod models;\npub mod mux;\n",
)

old_upscale_mux = r'''fn mux_command(video_only: &Path, source: &Path, output: &Path) -> Command {
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
        .arg("1:a?")
        .arg("-map_metadata")
        .arg("1")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg("-shortest");

    if output
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("mp4"))
    {
        command.arg("-movflags").arg("+faststart");
    }
    command.arg(output);
    command
}

'''
replace_required("src-tauri/src/core/ai_upscale.rs", old_upscale_mux, "")
replace_required(
    "src-tauri/src/core/ai_upscale.rs",
    "        mux_command(&video_only, input, output),\n",
    "        super::mux::build_ai_mux_command(&video_only, input, output, duration)?,\n",
)

old_interpolation_mux = r'''fn mux_command(video_only: &Path, source: &Path, output: &Path) -> Command {
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
        .arg("1:a?")
        .arg("-map_metadata")
        .arg("1")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg("-shortest");

    if output
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("mp4"))
    {
        command.arg("-movflags").arg("+faststart");
    }

    command.arg(output);
    command
}

'''
replace_required("src-tauri/src/core/interpolation.rs", old_interpolation_mux, "")
replace_required(
    "src-tauri/src/core/interpolation.rs",
    "        mux_command(&video_only, input, output),\n",
    "        super::mux::build_ai_mux_command(&video_only, input, output, duration)?,\n",
)

# Move bilingual UI strings into React/TypeScript rather than CSS pseudo-elements.
replace_required(
    "src/App.tsx",
    'import { open, save } from "@tauri-apps/plugin-dialog";\n',
    'import { open, save } from "@tauri-apps/plugin-dialog";\nimport { bi, ui } from "./copy";\n',
)

replace_required(
    "src/App.tsx",
    '''type NcnnRuntimeInfo = {\n  realesrgan: BinaryProbe;\n  realcugan: BinaryProbe;\n  rife: BinaryProbe;\n  model_dir: string | null;\n  models: ModelManifest[];\n};\n\ntype MediaProbe = {\n  path: string;\n  duration_seconds: number | null;\n  width: number | null;\n  height: number | null;\n  frame_rate: number | null;\n  video_codec: string | null;\n  pixel_format: string | null;\n  audio_codec: string | null;\n  container: string | null;\n};''',
    '''type ModelProbe = {\n  id: string;\n  available: boolean;\n  resolved_path: string | null;\n  detail: string;\n};\n\ntype NcnnRuntimeInfo = {\n  realesrgan: BinaryProbe;\n  realcugan: BinaryProbe;\n  rife: BinaryProbe;\n  model_dir: string | null;\n  models: ModelManifest[];\n  model_catalog: ModelManifest[];\n  model_probes: ModelProbe[];\n};\n\ntype MediaProbe = {\n  path: string;\n  duration_seconds: number | null;\n  width: number | null;\n  height: number | null;\n  frame_rate: number | null;\n  nominal_frame_rate: number | null;\n  frame_count: number | null;\n  variable_frame_rate: boolean;\n  video_codec: string | null;\n  pixel_format: string | null;\n  bits_per_raw_sample: number | null;\n  high_bit_depth: boolean;\n  hdr: boolean;\n  interlaced: boolean;\n  color_primaries: string | null;\n  color_transfer: string | null;\n  color_space: string | null;\n  audio_codec: string | null;\n  subtitle_streams: number;\n  attachment_streams: number;\n  container: string | null;\n};''',
)

replace_required(
    "src/App.tsx",
    '''const modes: Array<{ id: Mode; title: string; detail: string }> = [\n  { id: "fast", title: "Fast", detail: "NCNN/Vulkan · broad GPU support" },\n  { id: "quality", title: "Quality", detail: "TensorRT / CUDA · planned" },\n  { id: "restore", title: "AI Restore", detail: "Temporal restoration · planned" },\n];''',
    '''const modes: Array<{ id: Mode; title: string; detail: string }> = [\n  { id: "fast", title: bi("极速", "Fast"), detail: bi("NCNN/Vulkan · 广泛 GPU 支持", "NCNN/Vulkan · broad GPU support") },\n  { id: "quality", title: bi("高质量", "Quality"), detail: bi("TensorRT / CUDA · 规划中", "TensorRT / CUDA · planned") },\n  { id: "restore", title: bi("AI 修复", "AI Restore"), detail: bi("时序修复 · 规划中", "Temporal restoration · planned") },\n];''',
)
replace_required("src/App.tsx", 'return "Unknown";', "return ui.unknown;")

replace_required(
    "src/App.tsx",
    '''  const anyFastEngineAvailable = Boolean(\n    ncnnRuntime?.realesrgan.available || ncnnRuntime?.realcugan.available || ncnnRuntime?.rife.available,\n  );''',
    '''  const anyFastModelReady = upscaleModels.length > 0 || interpolationModels.length > 0;''',
)
replace_required(
    "src/App.tsx",
    '''  const transformedCodecAvailable =\n    codec !== "copy" &&\n    ((codec === "h264" && Boolean(runtime?.libx264)) ||\n      (codec === "h265" && Boolean(runtime?.libx265)));''',
    '''  const transformedCodecAvailable =\n    codec !== "copy" &&\n    ((codec === "h264" && Boolean(runtime?.libx264)) ||\n      (codec === "h265" && Boolean(runtime?.libx265)));\n  const aiMediaBlockedReason = media?.variable_frame_rate\n    ? ui.vfrBlocked\n    : media?.hdr || media?.high_bit_depth\n      ? ui.hdrBlocked\n      : media?.interlaced\n        ? ui.interlacedBlocked\n        : null;\n  const outputIsMkv = outputPath?.toLowerCase().endsWith(".mkv") ?? false;''',
)
replace_required(
    "src/App.tsx",
    '''      selectedModel &&\n      media.duration_seconds &&''',
    '''      selectedModel &&\n      !aiMediaBlockedReason &&\n      media.duration_seconds &&''',
)
replace_required(
    "src/App.tsx",
    '''      selectedRifeModel &&\n      media.duration_seconds &&''',
    '''      selectedRifeModel &&\n      !aiMediaBlockedReason &&\n      media.duration_seconds &&''',
)

# Job and action messages.
replace_required("src/App.tsx", 'beginJob("media", "Starting local FFmpeg validation pipeline…");', 'beginJob("media", bi("正在启动本地 FFmpeg 验证管线…", "Starting local FFmpeg validation pipeline…"));')
replace_required("src/App.tsx", 'beginJob("upscale", `Starting ${selectedModel.display_name}…`);', 'beginJob("upscale", bi(`正在启动 ${selectedModel.display_name}…`, `Starting ${selectedModel.display_name}…`));')
replace_required("src/App.tsx", 'beginJob("interpolation", `Starting ${selectedRifeModel.display_name}…`);', 'beginJob("interpolation", bi(`正在启动 ${selectedRifeModel.display_name}…`, `Starting ${selectedRifeModel.display_name}…`));')
replace_required("src/App.tsx", 'setJobMessage(`RIFE active · target ${response.output_frame_rate.toFixed(3)} FPS`);', 'setJobMessage(bi(`RIFE 已启用 · 目标 ${response.output_frame_rate.toFixed(3)} FPS`, `RIFE active · target ${response.output_frame_rate.toFixed(3)} FPS`));')
replace_required("src/App.tsx", 'if (!cancelled) setJobMessage("The processing job is no longer active.");\n      else setJobMessage("Cancelling…");', 'if (!cancelled) setJobMessage(ui.inactiveJob);\n      else setJobMessage(ui.cancelling);')

# Main visible controls.
for old, new in [
    ('<div className="local-badge">Local processing</div>', '<div className="local-badge">{ui.localProcessing}</div>'),
    ('{media.frame_rate ? `${media.frame_rate.toFixed(3)} FPS` : "FPS unknown"}', '{media.frame_rate ? `${media.frame_rate.toFixed(3)} FPS` : bi("帧率未知", "FPS unknown")}'),
    ('{media.video_codec?.toUpperCase() ?? "Codec unknown"}', '{media.video_codec?.toUpperCase() ?? bi("编码未知", "Codec unknown")}'),
    ('<p className="target-resolution">SR target · {targetResolution}</p>', '<p className="target-resolution">{ui.srTarget} · {targetResolution}</p>'),
    ('{probing ? "Inspecting…" : "Choose another video"}', '{probing ? ui.inspecting : ui.chooseAnother}'),
    ('<h2>Select a video</h2>', '<h2>{ui.selectVideo}</h2>'),
    ('{probing ? "Inspecting…" : "Select video"}', '{probing ? ui.inspecting : ui.selectVideo}'),
    ('<p className="error-message">ffprobe is not available in the managed runtime or PATH.</p>', '<p className="error-message">{ui.ffprobeMissing}</p>'),
    ('? anyFastEngineAvailable\n                        ? "Ready for local AI processing"\n                        : "Media pipeline ready · AI runtime missing"', '? anyFastModelReady\n                        ? ui.readyAi\n                        : ui.mediaReadyAiMissing'),
    ('aria-label="Processing progress"', 'aria-label={bi("处理进度", "Processing progress")}'),
    ('<span>{formatDuration(outTime)} processed</span>', '<span>{formatDuration(outTime)} · {ui.processed}</span>'),
    ('? "NCNN upscale chunks"', '? bi("NCNN 超分分块", "NCNN upscale chunks")'),
    (': jobKind === "interpolation"\n                        ? "RIFE overlap chunks"\n                        : "Waiting for FFmpeg"', ': jobKind === "interpolation"\n                        ? bi("RIFE 重叠分块", "RIFE overlap chunks")\n                        : bi("等待 FFmpeg", "Waiting for FFmpeg")'),
    ('<span>Enhancement mode</span>', '<span>{ui.enhancementMode}</span>'),
    ('<small>Engine-aware</small>', '<small>{ui.engineAware}</small>'),
    ('<span>Enhancement</span>', '<span>{ui.enhancement}</span>'),
    ('"Planned backend"', 'ui.plannedBackend'),
    ('<p className="subsection-title">Super resolution</p>', '<p className="subsection-title">{ui.superResolution}</p>'),
    ('<label className="field-label" htmlFor="model">Upscale model</label>', '<label className="field-label" htmlFor="model">{ui.upscaleModel}</label>'),
    ('<option value="">No Fast-mode upscale profiles</option>', '<option value="">{ui.noUpscaleProfiles}</option>'),
    ('<p className="subsection-title">Frame interpolation</p>', '<p className="subsection-title">{ui.frameInterpolation}</p>'),
    ('<label className="field-label" htmlFor="rife-model">RIFE model</label>', '<label className="field-label" htmlFor="rife-model">{ui.rifeModel}</label>'),
    ('<option value="">No RIFE profiles</option>', '<option value="">{ui.noRifeProfiles}</option>'),
    ('<span>Target FPS</span>', '<span>{ui.targetFps}</span>'),
    ('<strong>{targetFps ? `${targetFps.toFixed(3)} FPS` : "Unknown"}</strong>', '<strong>{targetFps ? `${targetFps.toFixed(3)} FPS` : ui.unknown}</strong>'),
    ('<p className="pipeline-note">Scene protection · 0.42 threshold · one-frame chunk overlap</p>', '<p className="pipeline-note">{ui.sceneProtection}</p>'),
    ('<label className="field-label" htmlFor="codec">Video codec</label>', '<label className="field-label" htmlFor="codec">{ui.videoCodec}</label>'),
    ('<option value="copy">Copy source video stream · M1 only</option>', '<option value="copy">{ui.copyM1}</option>'),
    ('{outputPath ? fileName(outputPath) : "Choose output file"}', '{outputPath ? fileName(outputPath) : ui.chooseOutput}'),
    ('<button className="danger-button" type="button" onClick={cancelActiveJob}>Cancel processing</button>', '<button className="danger-button" type="button" onClick={cancelActiveJob}>{ui.cancelProcessing}</button>'),
    ('Enhance video · {selectedModel ? `${selectedModel.scale}×` : "NCNN"}', '{ui.enhanceVideo} · {selectedModel ? `${selectedModel.scale}×` : "NCNN"}'),
    ('Interpolate video · {targetFps ? `${targetFps.toFixed(3)} FPS` : "RIFE"}', '{ui.interpolateVideo} · {targetFps ? `${targetFps.toFixed(3)} FPS` : "RIFE"}'),
    ('Validate media pipeline', '{ui.validatePipeline}'),
    ('<p className="error-message">FFmpeg is required for video processing.</p>', '<p className="error-message">{ui.ffmpegRequired}</p>'),
    ('<p className="error-message">RIFE runtime is not available, so frame interpolation is disabled.</p>', '<p className="error-message">{ui.rifeMissing}</p>'),
    ('<p className="pipeline-note">AI transforms create new video frames, so H.264 or H.265 must be selected instead of stream copy.</p>', '<p className="pipeline-note">{ui.aiCopyUnsupported}</p>'),
    ('<p className="pipeline-note">Quality and AI Restore backends are intentionally disabled until their runtimes land.</p>', '<p className="pipeline-note">{ui.plannedModes}</p>'),
    ('<p className="pipeline-note">Fast mode keeps bounded frame chunks on disk and uses one persistent encoder for each job.</p>', '<p className="pipeline-note">{ui.fastPipeline}</p>'),
    ('<span>Source</span>', '<span>{ui.source}</span>'),
    ('<div><dt>Video</dt>', '<div><dt>{ui.video}</dt>'),
    ('<div><dt>Audio</dt>', '<div><dt>{ui.audio}</dt>'),
    ('<div><dt>Container</dt>', '<div><dt>{ui.container}</dt>'),
    ('<span>Media runtime</span>', '<span>{ui.mediaRuntime}</span>'),
    ('{runtime?.ffmpeg_available ? "Ready" : "Unavailable"}', '{runtime?.ffmpeg_available ? ui.ready : ui.unavailable}'),
    ('{runtime?.ffmpeg_available ? "Detected" : "Missing"}', '{runtime?.ffmpeg_available ? ui.detected : ui.missing}'),
    ('{runtime?.ffprobe_available ? "Detected" : "Missing"}', '{runtime?.ffprobe_available ? ui.detected : ui.missing}'),
    ('<span>AI runtime</span>', '<span>{ui.aiRuntime}</span>'),
    ('<div><dt>Models</dt>', '<div><dt>{ui.models}</dt>'),
    ('<div><dt>Catalog</dt>', '<div><dt>{ui.catalog}</dt>'),
    ('{ncnnRuntime?.model_dir ? "Managed directory" : "Engine default / not staged"}', '{ncnnRuntime?.model_dir ? ui.managedDirectory : ui.modelNotStaged}'),
    ('{ncnnRuntime ? `${ncnnRuntime.models.length} model profiles` : "Loading"}', '{ncnnRuntime ? `${ncnnRuntime.models.length}/${ncnnRuntime.model_catalog.length} ${bi("可运行", "ready")}` : ui.loading}'),
    ('<span>System</span>', '<span>{ui.system}</span>'),
    ('{hardware ? "Detected" : "Checking"}', '{hardware ? ui.detected : ui.checking}'),
    ('<div><dt>Platform</dt>', '<div><dt>{ui.platform}</dt>'),
    ('<div><dt>Memory</dt>', '<div><dt>{ui.memory}</dt>'),
    ('{hardware.gpu_hint ?? "NCNN will select Vulkan GPU automatically"}', '{hardware.gpu_hint ?? ui.gpuAuto}'),
    ('{hardwareError ?? "Reading local capabilities…"}', '{hardwareError ?? ui.readingCapabilities}'),
]:
    replace_required("src/App.tsx", old, new)

# Engine runtime error stays dynamic but becomes bilingual.
replace_required(
    "src/App.tsx",
    '''                  {engineLabel(selectedModel.engine)} runtime is not available. Add the managed runtime payload or configure the development PATH.''',
    '''                  {bi(`${engineLabel(selectedModel.engine)} 运行时不可用，请部署托管运行时/模型或配置开发 PATH。`, `${engineLabel(selectedModel.engine)} runtime is not available. Stage the managed runtime/model payload or configure the development PATH.`)}''',
)

# Add compatibility/subtitle notices near the Fast-mode pipeline note.
replace_required(
    "src/App.tsx",
    '''              {mode === "fast" && (\n                <p className="pipeline-note">{ui.fastPipeline}</p>\n              )}''',
    '''              {mode === "fast" && (\n                <p className="pipeline-note">{ui.fastPipeline}</p>\n              )}\n              {aiMediaBlockedReason && <p className="error-message">{aiMediaBlockedReason}</p>}\n              {media.subtitle_streams + media.attachment_streams > 0 && (\n                <p className="pipeline-note">\n                  {outputPath && !outputIsMkv ? ui.mp4SubtitleWarning : ui.mkvSubtitleHint}\n                </p>\n              )}''',
)

# Expand source diagnostics using the new ffprobe fields.
replace_required(
    "src/App.tsx",
    '''                <div><dt>{ui.container}</dt><dd>{media.container ?? "Unknown"}</dd></div>''',
    '''                <div><dt>{ui.container}</dt><dd>{media.container ?? ui.unknown}</dd></div>\n                <div><dt>{ui.frameTiming}</dt><dd>{media.variable_frame_rate ? "VFR" : "CFR"}{media.nominal_frame_rate ? ` · nominal ${media.nominal_frame_rate.toFixed(3)}` : ""}</dd></div>\n                <div><dt>{ui.colorDepth}</dt><dd>{media.hdr ? "HDR" : "SDR"} · {media.bits_per_raw_sample ? `${media.bits_per_raw_sample}-bit` : media.high_bit_depth ? ">8-bit" : "8-bit / unknown"}</dd></div>\n                <div><dt>{ui.auxiliaryStreams}</dt><dd>{media.subtitle_streams} / {media.attachment_streams}</dd></div>''',
)

# Avoid a misleading Ready state when only binaries exist but no real model payload is staged.
replace_required("src/App.tsx", "anyFastEngineAvailable", "anyFastModelReady", count=0) if False else None

# Remove CSS-generated user-facing bilingual copy; React now owns those strings.
glass = read("src/glass.css")
for block in [
    '''.local-badge::after {\n  content: " · 本地处理";\n  color: #b9d4ec;\n}\n\n''',
    '''.target-resolution::before {\n  content: "超分目标 / ";\n  color: #d4eaff;\n  font-weight: 650;\n}\n\n''',
]:
    if block not in glass:
        raise RuntimeError(f"src/glass.css: expected block missing: {block[:60]!r}")
    glass = glass.replace(block, "", 1)
start = glass.find("/* Bilingual presentation layer: Chinese first, existing English remains as the second line. */")
end = glass.find("/* Keep the bilingual controls compact on the narrow right rail. */")
if start < 0 or end < 0 or end <= start:
    raise RuntimeError("src/glass.css: bilingual pseudo-element section markers not found")
glass = glass[:start] + "/* Bilingual user-facing copy now lives in React/TypeScript for accessibility and testability. */\n\n" + glass[end:]
write("src/glass.css", glass)

# Apply a production CSP while keeping Tauri IPC, Vite dev HMR, and future local media preview schemes available.
conf_path = ROOT / "src-tauri/tauri.conf.json"
conf = json.loads(conf_path.read_text(encoding="utf-8"))
conf["app"]["security"]["csp"] = (
    "default-src 'self'; "
    "connect-src 'self' ipc: http://ipc.localhost http://localhost:1420 ws://localhost:1420; "
    "img-src 'self' asset: http://asset.localhost data: blob:; "
    "media-src 'self' asset: http://asset.localhost data: blob:; "
    "style-src 'self' 'unsafe-inline'; "
    "font-src 'self' data:; "
    "script-src 'self'"
)
conf_path.write_text(json.dumps(conf, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

# CI/release should consume the generated lockfiles after this bootstrap commit lands.
replace_required(".github/workflows/ci.yml", "run: npm install", "run: npm ci")
replace_required(".github/workflows/ci.yml", "run: cargo check", "run: cargo check --locked")
replace_required(".github/workflows/ci.yml", "run: cargo test --lib", "run: cargo test --lib --locked")
replace_required(".github/workflows/windows-release.yml", "run: npm install", "run: npm ci")
replace_required(".github/workflows/windows-release.yml", "run: cargo test --lib", "run: cargo test --lib --locked")
replace_required(
    ".github/workflows/windows-release.yml",
    '      - "package.json"\n',
    '      - "package.json"\n      - "package-lock.json"\n      - "src-tauri/Cargo.lock"\n',
)

# Sanity checks that catch regressions before the bot commit.
assert "-shortest" not in read("src-tauri/src/core/ai_upscale.rs")
assert "-shortest" not in read("src-tauri/src/core/interpolation.rs")
assert "::before" not in read("src/glass.css").split("Bilingual user-facing copy now lives", 1)[-1]
print("Release hardening source patches applied successfully.")
