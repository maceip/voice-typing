use crate::mic::{MicStartInfo, start_microphone};
use crate::model::{CurrentWiredModel, WiredModelPaths};
use anyhow::{Context, Result};
use async_trait::async_trait;
use cpal::Stream;
use voice_typing_core::{AsrResult, AsrService};
use sherpa_onnx::{
    OfflineMoonshineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, SileroVadModelConfig,
    VadModelConfig, VoiceActivityDetector,
};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

pub struct DesktopSherpaAsrService {
    configured_paths: Option<WiredModelPaths>,
    results_tx: broadcast::Sender<AsrResult>,
    worker: Option<JoinHandle<()>>,
    worker_tx: Option<Arc<WorkerQueue>>,
    mic_stream: Option<Stream>,
    stop_flag: Arc<AtomicBool>,
    telemetry: Arc<SessionTelemetry>,
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
            telemetry: Arc::new(SessionTelemetry::new()),
        }
    }

    pub fn session_health(&self) -> SessionHealth {
        self.telemetry.snapshot(self.worker.is_some())
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
        self.telemetry.mark_session_start();
        let stop = Arc::clone(&self.stop_flag);
        let queue = Arc::new(WorkerQueue::default());
        let results_tx = self.results_tx.clone();
        let worker_paths = paths.clone();
        let worker_queue = Arc::clone(&queue);
        let telemetry = Arc::clone(&self.telemetry);

        let worker = thread::spawn(move || {
            let Ok(mut session) = StreamingSession::new(&worker_paths) else {
                eprintln!("[ASR] failed to start streaming session");
                telemetry.record_error("failed to start streaming session".to_owned());
                return;
            };

            while let Some((command, overflowed)) = worker_queue.recv() {
                if overflowed {
                    eprintln!(
                        "[ASR] audio queue overflowed; dropping stale audio to recover latency"
                    );
                    match StreamingSession::new(&worker_paths) {
                        Ok(new_session) => session = new_session,
                        Err(err) => {
                            telemetry.record_error(format!(
                                "failed to reset streaming session after overflow: {err}"
                            ));
                            eprintln!(
                                "[ASR] failed to reset streaming session after overflow: {err}"
                            );
                            continue;
                        }
                    }
                }

                match command {
                    WorkerCommand::Audio(chunk) => {
                        telemetry.mark_audio();
                        for result in session.push_audio(&chunk) {
                            telemetry.mark_result();
                            let _ = results_tx.send(result);
                        }
                    }
                    WorkerCommand::Stop => break,
                }
            }

            for result in session.finish() {
                telemetry.mark_result();
                let _ = results_tx.send(result);
            }
        });

        let stop_for_callback = Arc::clone(&stop);
        let queue_for_callback = Arc::clone(&queue);
        let telemetry_for_audio = Arc::clone(&self.telemetry);
        let telemetry_for_error = Arc::clone(&self.telemetry);
        let (stream, info) = start_microphone(
            paths.sample_rate,
            move |chunk| {
                if !stop_for_callback.load(Ordering::SeqCst) {
                    telemetry_for_audio.mark_audio();
                    queue_for_callback.push_audio(chunk);
                }
            },
            move |message| telemetry_for_error.record_error(message),
        )?;
        log_mic_start(&info);

        self.worker_tx = Some(queue);
        self.worker = Some(worker);
        self.mic_stream = Some(stream);
        Ok(())
    }

    fn stop_real_time_session(&mut self) -> Result<()> {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.telemetry.mark_stopped();
        self.mic_stream.take();

        if let Some(tx) = self.worker_tx.take() {
            tx.push_stop();
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

#[derive(Default)]
struct WorkerQueue {
    state: Mutex<WorkerQueueState>,
    ready: Condvar,
}

#[derive(Default)]
struct WorkerQueueState {
    items: VecDeque<WorkerCommand>,
    overflowed: bool,
}

impl WorkerQueue {
    const MAX_CHUNKS: usize = 48;
    const TRIM_TO_CHUNKS: usize = 24;

    fn push_audio(&self, chunk: Vec<f32>) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        if state.items.len() >= Self::MAX_CHUNKS {
            while state.items.len() > Self::TRIM_TO_CHUNKS {
                state.items.pop_front();
                state.overflowed = true;
            }
        }

        state.items.push_back(WorkerCommand::Audio(chunk));
        self.ready.notify_one();
    }

    fn push_stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.items.push_back(WorkerCommand::Stop);
            self.ready.notify_one();
        }
    }

    fn recv(&self) -> Option<(WorkerCommand, bool)> {
        let mut state = self.state.lock().ok()?;

        loop {
            if let Some(command) = state.items.pop_front() {
                let overflowed = std::mem::take(&mut state.overflowed);
                return Some((command, overflowed));
            }

            state = self.ready.wait(state).ok()?;
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionHealth {
    pub worker_running: bool,
    pub last_audio_age: Option<Duration>,
    pub last_result_age: Option<Duration>,
    pub last_error: Option<String>,
}

struct SessionTelemetry {
    last_audio_at: Mutex<Option<Instant>>,
    last_result_at: Mutex<Option<Instant>>,
    last_error: Mutex<Option<String>>,
}

impl SessionTelemetry {
    fn new() -> Self {
        Self {
            last_audio_at: Mutex::new(None),
            last_result_at: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }

    fn mark_session_start(&self) {
        let now = Instant::now();
        if let Ok(mut value) = self.last_audio_at.lock() {
            *value = Some(now);
        }
        if let Ok(mut value) = self.last_result_at.lock() {
            *value = Some(now);
        }
        if let Ok(mut value) = self.last_error.lock() {
            *value = None;
        }
    }

    fn mark_audio(&self) {
        if let Ok(mut value) = self.last_audio_at.lock() {
            *value = Some(Instant::now());
        }
    }

    fn mark_result(&self) {
        if let Ok(mut value) = self.last_result_at.lock() {
            *value = Some(Instant::now());
        }
    }

    fn record_error(&self, error: String) {
        if let Ok(mut value) = self.last_error.lock() {
            *value = Some(error);
        }
    }

    fn mark_stopped(&self) {
        if let Ok(mut value) = self.last_error.lock() {
            *value = None;
        }
    }

    fn snapshot(&self, worker_running: bool) -> SessionHealth {
        let now = Instant::now();
        let last_audio_age = self
            .last_audio_at
            .lock()
            .ok()
            .and_then(|value| *value)
            .map(|value| now.saturating_duration_since(value));
        let last_result_age = self
            .last_result_at
            .lock()
            .ok()
            .and_then(|value| *value)
            .map(|value| now.saturating_duration_since(value));
        let last_error = self.last_error.lock().ok().and_then(|value| value.clone());

        SessionHealth {
            worker_running,
            last_audio_age,
            last_result_age,
            last_error,
        }
    }
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
    config.model_config.num_threads = std::thread::available_parallelism()
        .map(|value| value.get().clamp(2, 6) as i32)
        .unwrap_or(4);
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
            threshold: 0.45,
            min_silence_duration: 0.05,
            min_speech_duration: 0.05,
            max_speech_duration: 1.8,
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
    let peak_gain = 0.92 / peak;
    let rms_gain = if rms > 1e-6 { 0.08 / rms } else { peak_gain };
    let gain = peak_gain.min(rms_gain).clamp(0.8, 12.0);

    for sample in &mut centered {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }

    // Give the decoder a tiny silence tail so final phonemes are less likely to clip.
    centered.extend(std::iter::repeat_n(0.0, 800));

    centered
}
