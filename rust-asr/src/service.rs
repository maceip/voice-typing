use crate::mic::{MicStartInfo, start_microphone};
use crate::model::{CurrentWiredModel, WiredModelPaths};
use anyhow::{Context, Result};
use async_trait::async_trait;
use cpal::Stream;
use daydream_core::{AsrResult, AsrService};
use sherpa_onnx::{
    OfflineMoonshineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, SileroVadModelConfig,
    VadModelConfig, VoiceActivityDetector,
};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

pub struct DesktopSherpaAsrService {
    configured_paths: Option<WiredModelPaths>,
    results_tx: broadcast::Sender<AsrResult>,
    worker: Option<JoinHandle<()>>,
    worker_tx: Option<std::sync::mpsc::Sender<WorkerCommand>>,
    mic_stream: Option<Stream>,
    stop_flag: Arc<AtomicBool>,
}

impl DesktopSherpaAsrService {
    pub fn new() -> Self {
        let (results_tx, _results_rx) = broadcast::channel(256);
        Self {
            configured_paths: None,
            results_tx,
            worker: None,
            worker_tx: None,
            mic_stream: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn initialize_blocking(&mut self, model_path: &str) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to build Tokio runtime")?;
        runtime.block_on(self.initialize(model_path))
    }

    pub fn transcribe_wav(&self, wav_path: impl AsRef<Path>) -> Result<AsrResult> {
        let paths = self
            .configured_paths
            .as_ref()
            .context("ASR service not initialized")?;

        let (samples, sample_rate) = read_wav_mono_f32(wav_path.as_ref())?;

        let recognizer = create_recognizer(paths)?;
        let samples = enhance_for_asr(&samples);
        let stream = recognizer.create_stream();
        stream.accept_waveform(sample_rate as i32, &samples);
        recognizer.decode(&stream);
        let result = stream
            .get_result()
            .context("recognizer returned no result")?;

        Ok(AsrResult {
            text: result.text.trim().to_owned(),
            confidence: 0.9,
            is_final: true,
        })
    }
}

fn read_wav_mono_f32(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open wav file {}", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;

    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to decode float wav samples")?,
        hound::SampleFormat::Int => {
            let max = ((1i64 << (spec.bits_per_sample - 1)) - 1) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / max))
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("failed to decode integer wav samples")?
        }
    };

    let mono = if channels <= 1 {
        raw
    } else {
        raw.chunks(channels)
            .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
            .collect()
    };

    let samples = if spec.sample_rate == 16_000 {
        mono
    } else {
        resample_linear(&mono, spec.sample_rate, 16_000)
    };

    Ok((samples, 16_000))
}

fn resample_linear(input: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
    if input.is_empty() || input_rate == output_rate {
        return input.to_vec();
    }

    let ratio = output_rate as f64 / input_rate as f64;
    let output_len = ((input.len() as f64) * ratio).round() as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let position = i as f64 / ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(input.len().saturating_sub(1));
        let frac = (position - left as f64) as f32;
        let left_sample = input[left];
        let right_sample = input[right];
        output.push(left_sample + (right_sample - left_sample) * frac);
    }

    output
}

impl Default for DesktopSherpaAsrService {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DesktopSherpaAsrService {
    fn drop(&mut self) {
        let _ = self.stop_real_time_session();
    }
}

#[async_trait]
impl AsrService for DesktopSherpaAsrService {
    async fn initialize(&mut self, model_path: &str) -> Result<()> {
        let root =
            std::env::current_dir().context("failed to resolve current working directory")?;
        let paths = WiredModelPaths {
            model_dir: root.join(model_path),
            vad_model: root.join(CurrentWiredModel::VAD_PATH),
            sample_rate: 16_000,
        };

        for required in [
            "encoder_model.ort",
            "decoder_model_merged.ort",
            "tokens.txt",
        ] {
            let required_path = paths.model_dir.join(required);
            required_path
                .metadata()
                .with_context(|| format!("missing model file {}", required_path.display()))?;
        }
        paths
            .vad_model
            .metadata()
            .with_context(|| format!("missing VAD model {}", paths.vad_model.display()))?;

        self.configured_paths = Some(paths);
        Ok(())
    }

    fn start_real_time_session(&mut self) -> Result<()> {
        if self.worker.is_some() {
            return Ok(());
        }

        let paths = self
            .configured_paths
            .clone()
            .context("ASR service not initialized")?;

        self.stop_flag.store(false, Ordering::SeqCst);
        let stop = Arc::clone(&self.stop_flag);
        let (tx, rx) = std::sync::mpsc::channel::<WorkerCommand>();
        let results_tx = self.results_tx.clone();
        let worker_paths = paths.clone();

        let worker = thread::spawn(move || {
            let Ok(mut session) = StreamingSession::new(&worker_paths) else {
                eprintln!("[ASR] failed to start streaming session");
                return;
            };

            while let Ok(command) = rx.recv() {
                match command {
                    WorkerCommand::Audio(chunk) => {
                        for result in session.push_audio(&chunk) {
                            let _ = results_tx.send(result);
                        }
                    }
                    WorkerCommand::Stop => break,
                }
            }

            for result in session.finish() {
                let _ = results_tx.send(result);
            }
        });

        let stop_for_callback = Arc::clone(&stop);
        let tx_for_callback = tx.clone();
        let (stream, info) = start_microphone(paths.sample_rate, move |chunk| {
            if !stop_for_callback.load(Ordering::SeqCst) {
                let _ = tx_for_callback.send(WorkerCommand::Audio(chunk));
            }
        })?;
        log_mic_start(&info);

        self.worker_tx = Some(tx);
        self.worker = Some(worker);
        self.mic_stream = Some(stream);
        Ok(())
    }

    fn stop_real_time_session(&mut self) -> Result<()> {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.mic_stream.take();

        if let Some(tx) = self.worker_tx.take() {
            let _ = tx.send(WorkerCommand::Stop);
        }

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }

        Ok(())
    }

    fn subscribe_results(&self) -> broadcast::Receiver<AsrResult> {
        self.results_tx.subscribe()
    }
}

enum WorkerCommand {
    Audio(Vec<f32>),
    Stop,
}

struct StreamingSession {
    recognizer: OfflineRecognizer,
    vad: VoiceActivityDetector,
    sample_rate: u32,
    last_emitted: Option<(String, Instant)>,
}

impl StreamingSession {
    fn new(paths: &WiredModelPaths) -> Result<Self> {
        Ok(Self {
            recognizer: create_recognizer(paths)?,
            vad: create_vad(paths)?,
            sample_rate: paths.sample_rate,
            last_emitted: None,
        })
    }

    fn push_audio(&mut self, chunk: &[f32]) -> Vec<AsrResult> {
        self.vad.accept_waveform(chunk);
        self.drain_ready_segments()
    }

    fn finish(&mut self) -> Vec<AsrResult> {
        self.vad.flush();
        self.drain_ready_segments()
    }

    fn drain_ready_segments(&mut self) -> Vec<AsrResult> {
        let mut results = Vec::new();

        while !self.vad.is_empty() {
            let Some(segment) = self.vad.front() else {
                break;
            };
            self.vad.pop();

            if segment.samples().is_empty() {
                continue;
            }

            if let Some(result) = self.decode_segment(segment.samples()) {
                results.push(result);
            }
        }

        results
    }

    fn decode_segment(&mut self, samples: &[f32]) -> Option<AsrResult> {
        let enhanced = enhance_for_asr(samples);
        if enhanced.is_empty() {
            return None;
        }

        let stream = self.recognizer.create_stream();
        stream.accept_waveform(self.sample_rate as i32, &enhanced);
        self.recognizer.decode(&stream);
        let text = stream
            .get_result()
            .map(|result| result.text.trim().to_owned())
            .unwrap_or_default();

        if text.is_empty() {
            return None;
        }

        let now = Instant::now();
        if let Some((last_text, last_at)) = &self.last_emitted
            && *last_text == text
            && now.duration_since(*last_at) <= Duration::from_secs(2)
        {
            return None;
        }

        self.last_emitted = Some((text.clone(), now));

        Some(AsrResult {
            text,
            confidence: 0.9,
            is_final: true,
        })
    }
}

fn create_recognizer(paths: &WiredModelPaths) -> Result<OfflineRecognizer> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.tokens = Some(paths.model_dir.join("tokens.txt").display().to_string());
    config.model_config.num_threads = 4;
    config.model_config.debug = false;
    config.model_config.moonshine = OfflineMoonshineModelConfig {
        encoder: Some(
            paths
                .model_dir
                .join("encoder_model.ort")
                .display()
                .to_string(),
        ),
        merged_decoder: Some(
            paths
                .model_dir
                .join("decoder_model_merged.ort")
                .display()
                .to_string(),
        ),
        ..OfflineMoonshineModelConfig::default()
    };

    OfflineRecognizer::create(&config)
        .ok_or_else(|| anyhow::anyhow!("failed to create current wired Moonshine recognizer"))
}

fn create_vad(paths: &WiredModelPaths) -> Result<VoiceActivityDetector> {
    let config = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(paths.vad_model.display().to_string()),
            threshold: 0.5,
            min_silence_duration: 0.25,
            min_speech_duration: 0.1,
            max_speech_duration: 10.0,
            window_size: 512,
        },
        sample_rate: paths.sample_rate as i32,
        num_threads: 1,
        provider: Some("cpu".to_owned()),
        debug: false,
        ..VadModelConfig::default()
    };

    VoiceActivityDetector::create(&config, 30.0)
        .ok_or_else(|| anyhow::anyhow!("failed to create silero VAD"))
}

fn log_mic_start(info: &MicStartInfo) {
    eprintln!(
        "[ASR] input device='{}' format={:?} rate={}Hz channels={} target={}Hz resampling={} downmixing={}",
        info.device_name,
        info.sample_format,
        info.input_sample_rate,
        info.input_channels,
        info.target_sample_rate,
        info.resampling,
        info.downmixing
    );
}

fn enhance_for_asr(samples: &[f32]) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let mean = samples.iter().copied().sum::<f32>() / samples.len() as f32;
    let mut centered: Vec<f32> = samples.iter().map(|s| *s - mean).collect();

    let peak = centered
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0_f32, f32::max);

    if peak <= 1e-6 {
        return centered;
    }

    let rms = (centered.iter().map(|s| s * s).sum::<f32>() / centered.len() as f32).sqrt();
    let peak_gain = 0.85 / peak;
    let rms_gain = if rms > 1e-6 { 0.12 / rms } else { peak_gain };
    let gain = peak_gain.min(rms_gain).clamp(1.0, 20.0);

    for sample in &mut centered {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }

    centered
}
