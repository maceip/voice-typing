use anyhow::{Context, Result, anyhow};
use bzip2::read::BzDecoder;
use reqwest::blocking::Client;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tar::Archive;
use voice_typing_asr::CurrentWiredModel;

const MODEL_REPO: &str = "https://huggingface.co/csukuangfj2/sherpa-onnx-moonshine-base-en-quantized-2026-02-27/resolve/main";
const MODEL_ARCHIVE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-base-en-quantized-2026-02-27.tar.bz2";
const VAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx";

#[derive(Debug, Default)]
pub struct DownloadProgress {
    total_files: AtomicU64,
    completed_files: AtomicU64,
    current_total_bytes: AtomicU64,
    current_downloaded_bytes: AtomicU64,
    running: AtomicBool,
}

impl DownloadProgress {
    pub fn start(&self, total_files: u64) {
        self.total_files.store(total_files, Ordering::Relaxed);
        self.completed_files.store(0, Ordering::Relaxed);
        self.current_total_bytes.store(0, Ordering::Relaxed);
        self.current_downloaded_bytes.store(0, Ordering::Relaxed);
        self.running.store(true, Ordering::Relaxed);
    }

    pub fn begin_file(&self, total_bytes: Option<u64>) {
        self.current_downloaded_bytes.store(0, Ordering::Relaxed);
        self.current_total_bytes
            .store(total_bytes.unwrap_or(0), Ordering::Relaxed);
    }

    pub fn add_bytes(&self, count: u64) {
        self.current_downloaded_bytes
            .fetch_add(count, Ordering::Relaxed);
    }

    pub fn finish_file(&self) {
        self.completed_files.fetch_add(1, Ordering::Relaxed);
        self.current_downloaded_bytes.store(0, Ordering::Relaxed);
        self.current_total_bytes.store(0, Ordering::Relaxed);
    }

    pub fn finish(&self) {
        self.running.store(false, Ordering::Relaxed);
        self.current_downloaded_bytes.store(0, Ordering::Relaxed);
        self.current_total_bytes.store(0, Ordering::Relaxed);
    }

    pub fn fraction(&self) -> f32 {
        let total = self.total_files.load(Ordering::Relaxed).max(1);
        let completed = self.completed_files.load(Ordering::Relaxed).min(total);
        let current_total = self.current_total_bytes.load(Ordering::Relaxed);
        let current_done = self.current_downloaded_bytes.load(Ordering::Relaxed);
        let current_fraction = if current_total > 0 {
            (current_done as f32 / current_total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        ((completed as f32) + current_fraction) / total as f32
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

pub async fn ensure_auto_model(progress: Arc<DownloadProgress>) -> Result<()> {
    tokio::task::spawn_blocking(move || ensure_auto_model_blocking(progress))
        .await
        .map_err(|err| anyhow!("model download worker failed: {err}"))?
}

fn ensure_auto_model_blocking(progress: Arc<DownloadProgress>) -> Result<()> {
    if CurrentWiredModel::auto_assets_ready() {
        progress.finish();
        return Ok(());
    }

    fs::create_dir_all(CurrentWiredModel::auto_models_root())
        .context("failed to create model cache directory")?;

    progress.start(4);

    let client = Client::builder()
        .user_agent("voice-typing/0.1")
        .build()
        .context("failed to build HTTP client")?;

    let model_dir = CurrentWiredModel::auto_model_dir();
    fs::create_dir_all(&model_dir).with_context(|| {
        format!(
            "failed to create auto model directory {}",
            model_dir.display()
        )
    })?;

    match download_model_files(&client, &progress, &model_dir) {
        Ok(()) => {}
        Err(primary_err) => {
            download_model_archive(&client, &progress, &model_dir).with_context(|| {
                format!("failed primary file download and archive fallback: {primary_err}")
            })?;
        }
    }

    ensure_vad_file(&client, &progress)?;

    if !CurrentWiredModel::auto_assets_ready() {
        copy_bundled_assets(&progress)
            .context("downloaded assets were incomplete and bundled fallback failed")?;
    }

    if !CurrentWiredModel::auto_assets_ready() {
        anyhow::bail!(
            "auto model cache is still incomplete under {}",
            CurrentWiredModel::auto_models_root().display()
        );
    }

    progress.finish();
    Ok(())
}

fn download_model_files(
    client: &Client,
    progress: &DownloadProgress,
    model_dir: &Path,
) -> Result<()> {
    for file_name in [
        "encoder_model.ort",
        "decoder_model_merged.ort",
        "tokens.txt",
    ] {
        let target = model_dir.join(file_name);
        if target.exists() {
            progress.finish_file();
            continue;
        }

        let url = format!("{MODEL_REPO}/{file_name}");
        download_to_path(client, progress, &url, &target)?;
        progress.finish_file();
    }

    Ok(())
}

fn download_model_archive(
    client: &Client,
    progress: &DownloadProgress,
    model_dir: &Path,
) -> Result<()> {
    let archive_path = CurrentWiredModel::auto_models_root()
        .join(format!("{}.tar.bz2", CurrentWiredModel::MODEL_NAME));
    download_to_path(client, progress, MODEL_ARCHIVE_URL, &archive_path)?;

    let file = File::open(&archive_path).with_context(|| {
        format!(
            "failed to open downloaded archive {}",
            archive_path.display()
        )
    })?;
    let decoder = BzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive
        .unpack(CurrentWiredModel::auto_models_root())
        .with_context(|| {
            format!(
                "failed to extract model archive into {}",
                CurrentWiredModel::auto_models_root().display()
            )
        })?;

    for required in [
        "encoder_model.ort",
        "decoder_model_merged.ort",
        "tokens.txt",
    ] {
        let path = model_dir.join(required);
        path.metadata()
            .with_context(|| format!("archive did not provide {}", path.display()))?;
    }

    progress.finish_file();
    Ok(())
}

fn ensure_vad_file(client: &Client, progress: &DownloadProgress) -> Result<()> {
    let target = CurrentWiredModel::auto_vad_path();
    if target.exists() {
        progress.finish_file();
        return Ok(());
    }

    download_to_path(client, progress, VAD_URL, &target)?;
    progress.finish_file();
    Ok(())
}

fn download_to_path(
    client: &Client,
    progress: &DownloadProgress,
    url: &str,
    target: &Path,
) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed GET {url}"))?
        .error_for_status()
        .with_context(|| format!("upstream rejected {url}"))?;

    progress.begin_file(response.content_length());

    let part_path = target.with_extension(format!(
        "{}.part",
        target
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("download")
    ));

    let mut reader = response;
    let mut writer = File::create(&part_path)
        .with_context(|| format!("failed to create {}", part_path.display()))?;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = reader
            .read(&mut buffer)
            .with_context(|| format!("failed while reading {url}"))?;
        if count == 0 {
            break;
        }
        writer
            .write_all(&buffer[..count])
            .with_context(|| format!("failed writing {}", part_path.display()))?;
        progress.add_bytes(count as u64);
    }

    writer
        .flush()
        .with_context(|| format!("failed flushing {}", part_path.display()))?;
    fs::rename(&part_path, target).with_context(|| {
        format!(
            "failed to move downloaded file {} into place {}",
            part_path.display(),
            target.display()
        )
    })?;

    Ok(())
}

fn copy_bundled_assets(progress: &DownloadProgress) -> Result<()> {
    let roots = candidate_roots();
    let bundled_model = roots
        .iter()
        .map(|root| root.join(CurrentWiredModel::MODEL_DIR))
        .find(|path| path.exists())
        .context("unable to locate bundled model assets")?;
    let bundled_vad = roots
        .iter()
        .map(|root| root.join(CurrentWiredModel::VAD_PATH))
        .find(|path| path.exists())
        .context("unable to locate bundled VAD asset")?;

    fs::create_dir_all(CurrentWiredModel::auto_model_dir()).with_context(|| {
        format!(
            "failed to create bundled fallback directory {}",
            CurrentWiredModel::auto_model_dir().display()
        )
    })?;

    for file_name in [
        "encoder_model.ort",
        "decoder_model_merged.ort",
        "tokens.txt",
    ] {
        let source = bundled_model.join(file_name);
        let target = CurrentWiredModel::auto_model_dir().join(file_name);
        if !target.exists() {
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "failed to copy bundled asset {} -> {}",
                    source.display(),
                    target.display()
                )
            })?;
        }
    }
    if !CurrentWiredModel::auto_vad_path().exists() {
        fs::copy(&bundled_vad, CurrentWiredModel::auto_vad_path()).with_context(|| {
            format!(
                "failed to copy bundled VAD {} -> {}",
                bundled_vad.display(),
                CurrentWiredModel::auto_vad_path().display()
            )
        })?;
    }

    while progress.completed_files.load(Ordering::Relaxed) < 4 {
        progress.finish_file();
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "network smoke test"]
    fn auto_model_download_smoke() {
        let _ = fs::remove_dir_all(CurrentWiredModel::auto_model_dir());
        let _ = fs::remove_file(CurrentWiredModel::auto_vad_path());

        let progress = Arc::new(DownloadProgress::default());
        ensure_auto_model_blocking(progress).expect("auto downloader should repopulate cache");
        assert!(CurrentWiredModel::auto_assets_ready());
    }
}
