use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WiredModelPaths {
    pub model_dir: PathBuf,
    pub vad_model: PathBuf,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CurrentWiredModel;

impl CurrentWiredModel {
    pub const MODEL_NAME: &'static str = "sherpa-onnx-moonshine-base-en-quantized-2026-02-27";
    pub const MODEL_DIR: &'static str =
        "assets/models/sherpa-onnx-moonshine-base-en-quantized-2026-02-27";
    pub const VAD_PATH: &'static str = "assets/models/silero_vad.onnx";
    pub const VAD_FILE: &'static str = "silero_vad.onnx";
    pub const SAMPLE_RATE: u32 = 16_000;

    pub fn locate_from(root: impl AsRef<Path>) -> Result<WiredModelPaths> {
        let root = root.as_ref();
        let model_dir = root.join(Self::MODEL_DIR);
        let vad_model = root.join(Self::VAD_PATH);

        if !model_dir.exists() {
            anyhow::bail!("model directory not found: {}", model_dir.display());
        }
        if !vad_model.exists() {
            anyhow::bail!("VAD model not found: {}", vad_model.display());
        }

        for required in [
            "encoder_model.ort",
            "decoder_model_merged.ort",
            "tokens.txt",
        ] {
            let path = model_dir.join(required);
            path.metadata()
                .with_context(|| format!("missing current wired model file: {}", path.display()))?;
        }

        Ok(WiredModelPaths {
            model_dir,
            vad_model,
            sample_rate: Self::SAMPLE_RATE,
        })
    }

    pub fn voice_typing_home() -> PathBuf {
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .or_else(|| {
                let drive = std::env::var_os("HOMEDRIVE")?;
                let path = std::env::var_os("HOMEPATH")?;
                let mut buf = PathBuf::from(drive);
                buf.push(path);
                Some(buf)
            })
            .unwrap_or_else(|| PathBuf::from("."));

        base.join(".local").join("voice_typing")
    }

    pub fn auto_models_root() -> PathBuf {
        Self::voice_typing_home().join("models")
    }

    pub fn auto_model_dir() -> PathBuf {
        Self::auto_models_root().join(Self::MODEL_NAME)
    }

    pub fn auto_vad_path() -> PathBuf {
        Self::auto_models_root().join(Self::VAD_FILE)
    }

    pub fn auto_assets_ready() -> bool {
        Self::validate_paths(Self::auto_model_dir(), Self::auto_vad_path()).is_ok()
    }

    pub fn resolve_runtime_paths(model_path: impl AsRef<Path>) -> Result<WiredModelPaths> {
        let model_path = model_path.as_ref();

        if model_path.as_os_str().is_empty() {
            return Self::resolve_runtime_paths(Self::auto_model_dir());
        }

        if model_path.is_absolute() || model_path.exists() {
            return Self::resolve_model_dir(model_path);
        }

        for root in candidate_roots() {
            let candidate = root.join(model_path);
            if candidate.exists() {
                return Self::resolve_model_dir(candidate);
            }
        }

        Self::resolve_model_dir(model_path)
    }

    fn resolve_model_dir(model_dir: impl AsRef<Path>) -> Result<WiredModelPaths> {
        let model_dir = model_dir.as_ref().to_path_buf();
        let vad_candidates = [
            model_dir.join(Self::VAD_FILE),
            model_dir
                .parent()
                .map(|parent| parent.join(Self::VAD_FILE))
                .unwrap_or_else(Self::auto_vad_path),
            Self::auto_vad_path(),
        ];

        for root in candidate_roots() {
            vad_candidates
                .iter()
                .cloned()
                .chain(std::iter::once(root.join(Self::VAD_PATH)))
                .find_map(|vad_model| Self::validate_paths(model_dir.clone(), vad_model).ok())
                .map(Ok)
                .unwrap_or_else(|| {
                    Err(anyhow::anyhow!(
                        "model directory or VAD missing for {}",
                        model_dir.display()
                    ))
                })?;
        }

        let vad_model = vad_candidates
            .iter()
            .find(|candidate| candidate.exists())
            .cloned()
            .or_else(|| {
                candidate_roots()
                    .into_iter()
                    .map(|root| root.join(Self::VAD_PATH))
                    .find(|candidate| candidate.exists())
            })
            .context("unable to locate silero_vad.onnx")?;

        Self::validate_paths(model_dir, vad_model)
    }

    fn validate_paths(model_dir: PathBuf, vad_model: PathBuf) -> Result<WiredModelPaths> {
        if !model_dir.exists() {
            anyhow::bail!("model directory not found: {}", model_dir.display());
        }
        if !vad_model.exists() {
            anyhow::bail!("VAD model not found: {}", vad_model.display());
        }

        for required in [
            "encoder_model.ort",
            "decoder_model_merged.ort",
            "tokens.txt",
        ] {
            let path = model_dir.join(required);
            path.metadata()
                .with_context(|| format!("missing current wired model file: {}", path.display()))?;
        }

        Ok(WiredModelPaths {
            model_dir,
            vad_model,
            sample_rate: Self::SAMPLE_RATE,
        })
    }
}

fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(current_dir) = std::env::current_dir() {
        roots.push(current_dir);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.to_path_buf());
        }
    }

    roots
}
