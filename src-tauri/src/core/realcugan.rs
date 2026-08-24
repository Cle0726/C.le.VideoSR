use std::{
    path::{Path, PathBuf},
    process::Command,
};

use super::{
    engine::{DirectoryCliEngine, EngineDescriptor, EngineError, EngineKind, EnhancementEngine},
    models::ModelManifest,
    ncnn::{resolve_ncnn_binary, resolve_ncnn_model_dir},
};

pub struct RealCuganNcnnEngine {
    binary: PathBuf,
    model: ModelManifest,
    model_dir: Option<PathBuf>,
    noise_level: i32,
    syncgap_mode: u32,
    tile_size: u32,
    gpu_id: Option<u32>,
    tta: bool,
}

impl RealCuganNcnnEngine {
    pub fn new(binary: impl Into<PathBuf>, model: ModelManifest) -> Result<Self, EngineError> {
        if model.engine != "realcugan-ncnn-vulkan" {
            return Err(EngineError::Configuration(format!(
                "model {} targets engine {}, not realcugan-ncnn-vulkan",
                model.id, model.engine
            )));
        }
        if !matches!(model.scale, 1 | 2 | 3 | 4) {
            return Err(EngineError::Configuration(format!(
                "unsupported Real-CUGAN scale {} for model {}",
                model.scale, model.id
            )));
        }

        let requested = binary.into();
        let resolved = if requested.components().count() == 1 {
            resolve_ncnn_binary(&requested.to_string_lossy())
        } else {
            requested
        };

        let model_dir = resolve_model_directory(&resolved, &model.model_stem);
        Ok(Self {
            binary: resolved,
            model,
            model_dir,
            noise_level: -1,
            syncgap_mode: 3,
            tile_size: 0,
            gpu_id: None,
            tta: false,
        })
    }

    pub fn with_noise_level(mut self, level: i32) -> Result<Self, EngineError> {
        if !(-1..=3).contains(&level) {
            return Err(EngineError::Configuration(
                "Real-CUGAN noise level must be -1, 0, 1, 2 or 3".into(),
            ));
        }
        self.noise_level = level;
        Ok(self)
    }

    pub fn with_syncgap_mode(mut self, mode: u32) -> Result<Self, EngineError> {
        if mode > 3 {
            return Err(EngineError::Configuration(
                "Real-CUGAN syncgap mode must be between 0 and 3".into(),
            ));
        }
        self.syncgap_mode = mode;
        Ok(self)
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

    fn command(&self, input: &Path, output: &Path) -> Command {
        let mut command = Command::new(&self.binary);
        command
            .arg("-i").arg(input)
            .arg("-o").arg(output)
            .arg("-n").arg(self.noise_level.to_string())
            .arg("-s").arg(self.model.scale.to_string())
            .arg("-t").arg(self.tile_size.to_string())
            .arg("-c").arg(self.syncgap_mode.to_string())
            .arg("-f").arg("png");
        if let Some(model_dir) = &self.model_dir { command.arg("-m").arg(model_dir); }
        if let Some(gpu_id) = self.gpu_id { command.arg("-g").arg(gpu_id.to_string()); }
        if self.tta { command.arg("-x"); }
        command
    }
}

fn resolve_model_directory(binary: &Path, model_stem: &str) -> Option<PathBuf> {
    if let Some(root) = resolve_ncnn_model_dir() {
        if root.file_name().and_then(|v| v.to_str()) == Some(model_stem) && root.is_dir() {
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

impl EnhancementEngine for RealCuganNcnnEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: format!("realcugan-ncnn:{}", self.model.id),
            display_name: format!("{} · NCNN/Vulkan", self.model.display_name),
            kind: EngineKind::NcnnVulkan,
            available: Command::new(&self.binary).arg("-h").output().is_ok(),
            detail: Some(format!(
                "binary={} scale={} noise={} syncgap={} tile={} gpu={} tta={} models={}",
                self.binary.display(), self.model.scale, self.noise_level, self.syncgap_mode,
                self.tile_size,
                self.gpu_id.map(|v| v.to_string()).unwrap_or_else(|| "auto".into()),
                self.tta,
                self.model_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "engine-default".into())
            )),
        }
    }

    fn self_test(&self) -> Result<(), EngineError> {
        let output = Command::new(&self.binary).arg("-h").output()
            .map_err(|error| EngineError::Unavailable(format!("{}: {error}", self.binary.display())))?;
        if output.stdout.is_empty() && output.stderr.is_empty() && !output.status.success() {
            return Err(EngineError::Unavailable(format!(
                "{} returned status {} during self-test", self.binary.display(), output.status
            )));
        }
        Ok(())
    }

    fn process(&self, input: &Path, output: &Path) -> Result<(), EngineError> {
        if !input.exists() {
            return Err(EngineError::Execution(format!("input path does not exist: {}", input.display())));
        }
        self.self_test()?;
        let result = self.command(input, output).output()
            .map_err(|error| EngineError::Execution(error.to_string()))?;
        if result.status.success() { return Ok(()); }
        let stderr = String::from_utf8_lossy(&result.stderr);
        let message = stderr.lines().rev().find(|line| !line.trim().is_empty())
            .unwrap_or("Real-CUGAN NCNN process failed");
        Err(EngineError::Execution(message.to_string()))
    }
}

impl DirectoryCliEngine for RealCuganNcnnEngine {
    fn build_directory_command(&self, input: &Path, output: &Path) -> Command {
        self.command(input, output)
    }
}
