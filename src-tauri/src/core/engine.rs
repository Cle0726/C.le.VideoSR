use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    NcnnVulkan,
    TensorRt,
    PythonWorker,
    Ffmpeg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineDescriptor {
    pub id: String,
    pub display_name: String,
    pub kind: EngineKind,
    pub available: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("engine is unavailable: {0}")]
    Unavailable(String),
    #[error("invalid engine configuration: {0}")]
    Configuration(String),
    #[error("engine execution failed: {0}")]
    Execution(String),
}

pub trait EnhancementEngine: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;
    fn self_test(&self) -> Result<(), EngineError>;
    fn process(&self, input: &Path, output: &Path) -> Result<(), EngineError>;
}

/// CLI backends that can process a directory of decoded frames in one invocation.
///
/// This is intentionally separate from `EnhancementEngine`: future native-library,
/// TensorRT and Python-worker backends do not need to expose a process command.
pub trait DirectoryCliEngine: EnhancementEngine {
    fn build_directory_command(&self, input: &Path, output: &Path) -> Command;
}

pub struct EngineRegistry {
    engines: Vec<Box<dyn EnhancementEngine>>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self { engines: Vec::new() }
    }

    pub fn register<E>(&mut self, engine: E)
    where
        E: EnhancementEngine + 'static,
    {
        self.engines.push(Box::new(engine));
    }

    pub fn descriptors(&self) -> Vec<EngineDescriptor> {
        self.engines.iter().map(|engine| engine.descriptor()).collect()
    }

    pub fn get(&self, id: &str) -> Option<&dyn EnhancementEngine> {
        self.engines
            .iter()
            .find(|engine| engine.descriptor().id == id)
            .map(|engine| engine.as_ref())
    }
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}
