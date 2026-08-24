use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStageKind {
    Deinterlace,
    Deblock,
    Denoise,
    Deblur,
    SuperResolution,
    FrameInterpolation,
    GrainRestore,
    Encode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub kind: PipelineStageKind,
    pub enabled: bool,
    pub engine: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnhancementPipeline {
    pub stages: Vec<PipelineStage>,
}

impl EnhancementPipeline {
    pub fn fast_default() -> Self {
        Self {
            stages: vec![
                PipelineStage {
                    kind: PipelineStageKind::SuperResolution,
                    enabled: true,
                    engine: Some("ncnn-vulkan".into()),
                    model: Some("auto".into()),
                },
                PipelineStage {
                    kind: PipelineStageKind::Encode,
                    enabled: true,
                    engine: Some("ffmpeg".into()),
                    model: None,
                },
            ],
        }
    }
}
