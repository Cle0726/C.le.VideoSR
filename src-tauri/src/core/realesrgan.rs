use std::{
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    engine::{EngineDescriptor, EngineError, EngineKind, EnhancementEngine},
    models::ModelManifest,
    ncnn::{resolve_ncnn_binary, resolve_ncnn_model_dir},
};

pub struct RealEsrganNcnnEngine {
    binary: PathBuf,
    model: ModelManifest,
    model_dir: Option<PathBuf>,
    tile_size: u32,
    gpu_id: Option<u32>,
    tta: bool,
}

impl RealEsrganNcnnEngine {
    pub fn new(binary: impl Into<PathBuf>, model: ModelManifest) -> Result<Self, EngineError> {
        if model.engine != "realesrgan-ncnn-vulkan" {
            return Err(EngineError::Configuration(format!(
                "model {} targets engine {}, not realesrgan-ncnn-vulkan",
                model.id, model.engine
            )));
        }

        if !matches!(model.scale, 2 | 3 | 4) {
            return Err(EngineError::Configuration(format!(
                "unsupported Real-ESRGAN NCNN scale {} for model {}",
                model.scale, model.id
            )));
        }

        let requested_binary = binary.into();
        let resolved_binary = if requested_binary.components().count() == 1 {
            let program = requested_binary.to_string_lossy();
            resolve_ncnn_binary(&program)
        } else {
            requested_binary
        };

        Ok(Self {
            binary: resolved_binary,
            model,
            model_dir: resolve_ncnn_model_dir(),
            tile_size: 0,
            gpu_id: None,
            tta: false,
        })
    }

    pub fn with_model_dir(mut self, model_dir: impl Into<PathBuf>) -> Self {
        self.model_dir = Some(model_dir.into());
        self
    }

    pub fn with_tile_size(mut self, tile_size: u32) -> Result<Self, EngineError> {
        if tile_size != 0 && tile_size < 32 {
            return Err(EngineError::Configuration(
                "tile size must be 0 (auto) or at least 32".into(),
            ));
        }
        self.tile_size = tile_size;
        Ok(self)
    }

    pub fn with_gpu_id(mut self, gpu_id: u32) -> Self {
        self.gpu_id = Some(gpu_id);
        self
    }

    pub fn with_tta(mut self, enabled: bool) -> Self {
        self.tta = enabled;
        self
    }

    pub(crate) fn build_command(&self, input: &Path, output: &Path) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .arg("-i")
            .arg(input)
            .arg("-o")
            .arg(output)
            .arg("-s")
            .arg(self.model.scale.to_string())
            .arg("-t")
            .arg(self.tile_size.to_string())
            .arg("-n")
            .arg(&self.model.model_stem)
            .arg("-f")
            .arg("png");

        if let Some(model_dir) = &self.model_dir {
            command.arg("-m").arg(model_dir);
        }
        if let Some(gpu_id) = self.gpu_id {
            command.arg("-g").arg(gpu_id.to_string());
        }
        if self.tta {
            command.arg("-x");
        }

        command
    }
}

impl EnhancementEngine for RealEsrganNcnnEngine {
    fn descriptor(&self) -> EngineDescriptor {
        let available = Command::new(&self.binary).arg("-h").output().is_ok();
        EngineDescriptor {
            id: format!("realesrgan-ncnn:{}", self.model.id),
            display_name: format!("{} · NCNN/Vulkan", self.model.display_name),
            kind: EngineKind::NcnnVulkan,
            available,
            detail: Some(format!(
                "binary={} scale={} tile={} gpu={} tta={} models={}",
                self.binary.display(),
                self.model.scale,
                self.tile_size,
                self.gpu_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "auto".into()),
                self.tta,
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

        let output_result = self
            .build_command(input, output)
            .output()
            .map_err(|error| EngineError::Execution(error.to_string()))?;

        if output_result.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output_result.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Real-ESRGAN NCNN process failed");

        Err(EngineError::Execution(message.to_string()))
    }
}
