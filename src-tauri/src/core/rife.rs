use std::{
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    engine::{DirectoryCliEngine, EngineDescriptor, EngineError, EngineKind, EnhancementEngine},
    models::ModelManifest,
    ncnn::{resolve_ncnn_binary, resolve_ncnn_model_dir},
};

pub struct RifeNcnnEngine {
    binary: PathBuf,
    model: ModelManifest,
    model_dir: Option<PathBuf>,
    gpu_id: Option<u32>,
    spatial_tta: bool,
    temporal_tta: bool,
    uhd: bool,
}

impl RifeNcnnEngine {
    pub fn new(binary: impl Into<PathBuf>, model: ModelManifest) -> Result<Self, EngineError> {
        if model.engine != "rife-ncnn-vulkan" || model.task != "frame_interpolation" {
            return Err(EngineError::Configuration(format!(
                "model {} does not target RIFE frame interpolation",
                model.id
            )));
        }

        let multiplier = model.frame_multiplier.unwrap_or(2);
        if multiplier != 2 {
            return Err(EngineError::Configuration(format!(
                "M3 RIFE adapter currently supports 2x FPS only, got {}x for {}",
                multiplier, model.id
            )));
        }

        let requested = binary.into();
        let resolved = if requested.components().count() == 1 {
            resolve_ncnn_binary(&requested.to_string_lossy())
        } else {
            requested
        };

        Ok(Self {
            model_dir: resolve_model_directory(&resolved, &model.model_stem),
            binary: resolved,
            model,
            gpu_id: None,
            spatial_tta: false,
            temporal_tta: false,
            uhd: false,
        })
    }

    pub fn with_gpu_id(mut self, gpu_id: u32) -> Self {
        self.gpu_id = Some(gpu_id);
        self
    }

    pub fn with_spatial_tta(mut self, enabled: bool) -> Self {
        self.spatial_tta = enabled;
        self
    }

    pub fn with_temporal_tta(mut self, enabled: bool) -> Self {
        self.temporal_tta = enabled;
        self
    }

    pub fn with_uhd(mut self, enabled: bool) -> Self {
        self.uhd = enabled;
        self
    }

    fn command(&self, input: &Path, output: &Path) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .arg("-i")
            .arg(input)
            .arg("-o")
            .arg(output)
            .arg("-f")
            .arg("%08d.png");

        if let Some(model_dir) = &self.model_dir {
            command.arg("-m").arg(model_dir);
        }
        if let Some(gpu_id) = self.gpu_id {
            command.arg("-g").arg(gpu_id.to_string());
        }
        if self.spatial_tta {
            command.arg("-x");
        }
        if self.temporal_tta {
            command.arg("-z");
        }
        if self.uhd {
            command.arg("-u");
        }

        command
    }
}

fn resolve_model_directory(binary: &Path, model_stem: &str) -> Option<PathBuf> {
    if let Some(root) = resolve_ncnn_model_dir() {
        if root.file_name().and_then(|value| value.to_str()) == Some(model_stem) && root.is_dir() {
            return Some(root);
        }

        let nested = root.join(model_stem);
        if nested.is_dir() {
            return Some(nested);
        }
    }

    binary.parent().and_then(|parent| {
        let candidate = parent.join(model_stem);
        candidate.is_dir().then_some(candidate)
    })
}

impl EnhancementEngine for RifeNcnnEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: format!("rife-ncnn:{}", self.model.id),
            display_name: format!("{} · NCNN/Vulkan", self.model.display_name),
            kind: EngineKind::NcnnVulkan,
            available: Command::new(&self.binary).arg("-h").output().is_ok(),
            detail: Some(format!(
                "binary={} multiplier={} gpu={} spatial_tta={} temporal_tta={} uhd={} models={}",
                self.binary.display(),
                self.model.frame_multiplier.unwrap_or(2),
                self.gpu_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "auto".into()),
                self.spatial_tta,
                self.temporal_tta,
                self.uhd,
                self.model_dir
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "engine-default".into())
            )),
        }
    }

    fn self_test(&self) -> Result<(), EngineError> {
        let output = Command::new(&self.binary)
            .arg("-h")
            .output()
            .map_err(|error| {
                EngineError::Unavailable(format!("{}: {error}", self.binary.display()))
            })?;

        if output.stdout.is_empty() && output.stderr.is_empty() && !output.status.success() {
            return Err(EngineError::Unavailable(format!(
                "{} returned status {} during self-test",
                self.binary.display(),
                output.status
            )));
        }

        Ok(())
    }

    fn process(&self, input: &Path, output: &Path) -> Result<(), EngineError> {
        if !input.exists() {
            return Err(EngineError::Execution(format!(
                "input path does not exist: {}",
                input.display()
            )));
        }

        self.self_test()?;
        let result = self
            .command(input, output)
            .output()
            .map_err(|error| EngineError::Execution(error.to_string()))?;

        if result.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&result.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("RIFE NCNN process failed");
        Err(EngineError::Execution(message.to_string()))
    }
}

impl DirectoryCliEngine for RifeNcnnEngine {
    fn build_directory_command(&self, input: &Path, output: &Path) -> Command {
        self.command(input, output)
    }
}
