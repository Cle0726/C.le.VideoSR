use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Probing,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJob {
    pub id: String,
    pub input: PathBuf,
    pub output: PathBuf,
    pub status: JobStatus,
    pub progress: f32,
    pub current_frame: u64,
    pub total_frames: Option<u64>,
}

impl VideoJob {
    pub fn new(id: impl Into<String>, input: PathBuf, output: PathBuf) -> Self {
        Self {
            id: id.into(),
            input,
            output,
            status: JobStatus::Queued,
            progress: 0.0,
            current_frame: 0,
            total_frames: None,
        }
    }
}
