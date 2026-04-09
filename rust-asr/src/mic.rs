use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfigRange};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MicStartInfo {
    pub device_name: String,
    pub sample_format: SampleFormat,
    pub input_sample_rate: u32,
    pub input_channels: u16,
    pub target_sample_rate: u32,
    pub resampling: bool,
    pub downmixing: bool,
}

pub fn start_microphone<F>(target_sample_rate: u32, on_chunk: F) -> Result<(Stream, MicStartInfo)>
where
    F: FnMut(Vec<f32>) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("no default microphone input device found")?;
    let device_name = device
        .description()
        .map(|description| description.to_string())
        .unwrap_or_else(|_| String::from("<unknown input device>"));

    let selected = select_input_config(&device, target_sample_rate)?;
    let sample_format = selected.sample_format();
    let stream_config = selected.config();
    let input_rate = stream_config.sample_rate;
    let channels = stream_config.channels;

    let callback = AudioChunker::new(input_rate, channels, target_sample_rate, on_chunk);
    let err_fn = |err| eprintln!("[ASR] microphone stream error: {err}");

    let stream = match sample_format {
        SampleFormat::I8 => build_stream::<i8>(&device, &stream_config, callback, err_fn)?,
        SampleFormat::U8 => build_stream::<u8>(&device, &stream_config, callback, err_fn)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, callback, err_fn)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, callback, err_fn)?,
        SampleFormat::I32 => build_stream::<i32>(&device, &stream_config, callback, err_fn)?,
        SampleFormat::U32 => build_stream::<u32>(&device, &stream_config, callback, err_fn)?,
        SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, callback, err_fn)?,
        other => anyhow::bail!("unsupported microphone sample format: {other:?}"),
    };

    stream.play().context("failed to start microphone stream")?;
    Ok((
        stream,
        MicStartInfo {
            device_name,
            sample_format,
            input_sample_rate: input_rate,
            input_channels: channels,
            target_sample_rate,
            resampling: input_rate != target_sample_rate,
            downmixing: channels > 1,
        },
    ))
}

fn select_input_config(
    device: &cpal::Device,
    target_sample_rate: u32,
) -> Result<cpal::SupportedStreamConfig> {
    let configs = device
        .supported_input_configs()
        .context("failed to query supported microphone configs")?
        .collect::<Vec<SupportedStreamConfigRange>>();

    if configs.is_empty() {
        anyhow::bail!("microphone reports no supported input configs");
    }

    let chosen = configs
        .iter()
        .filter_map(|range| {
            let min = range.min_sample_rate();
            let max = range.max_sample_rate();
            if min <= target_sample_rate && target_sample_rate <= max {
                Some(range.clone().with_sample_rate(target_sample_rate))
            } else {
                None
            }
        })
        .min_by_key(|config| {
            let channel_penalty = if config.channels() == 1 { 0 } else { 1 };
            let format_penalty = match config.sample_format() {
                SampleFormat::I16 => 0,
                SampleFormat::F32 => 1,
                SampleFormat::U16 => 2,
                SampleFormat::I32 => 3,
                SampleFormat::U32 => 4,
                SampleFormat::I8 => 5,
                SampleFormat::U8 => 6,
                _ => 10,
            };
            (channel_penalty, format_penalty)
        })
        .unwrap_or_else(|| {
            device
                .default_input_config()
                .expect("default input config should exist after supported_input_configs")
        });

    Ok(chosen)
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    chunker: AudioChunker,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<Stream>
where
    T: SizedSample,
    f32: cpal::FromSample<T>,
{
    let shared = Arc::new(std::sync::Mutex::new(chunker));
    let state = Arc::clone(&shared);

    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if let Ok(mut chunker) = state.lock() {
                    chunker.push_interleaved(data.iter().map(|sample| sample.to_sample::<f32>()));
                }
            },
            err_fn,
            None,
        )
        .context("failed to build microphone input stream")
}

struct AudioChunker {
    input_sample_rate: u32,
    input_channels: u16,
    target_sample_rate: u32,
    on_chunk: Box<dyn FnMut(Vec<f32>) + Send>,
    pending: Vec<f32>,
}

impl AudioChunker {
    const CHUNK_SIZE: usize = 512;

    fn new<F>(
        input_sample_rate: u32,
        input_channels: u16,
        target_sample_rate: u32,
        on_chunk: F,
    ) -> Self
    where
        F: FnMut(Vec<f32>) + Send + 'static,
    {
        Self {
            input_sample_rate,
            input_channels,
            target_sample_rate,
            on_chunk: Box::new(on_chunk),
            pending: Vec::new(),
        }
    }

    fn push_interleaved<I>(&mut self, samples: I)
    where
        I: IntoIterator<Item = f32>,
    {
        let mono = downmix_to_mono(samples, self.input_channels as usize);
        let resampled = if self.input_sample_rate == self.target_sample_rate {
            mono
        } else {
            resample_linear(&mono, self.input_sample_rate, self.target_sample_rate)
        };

        self.pending.extend(resampled);

        while self.pending.len() >= Self::CHUNK_SIZE {
            let chunk = self.pending.drain(..Self::CHUNK_SIZE).collect::<Vec<_>>();
            (self.on_chunk)(chunk);
        }
    }
}

fn downmix_to_mono<I>(samples: I, channels: usize) -> Vec<f32>
where
    I: IntoIterator<Item = f32>,
{
    let samples = samples.into_iter().collect::<Vec<_>>();
    if channels <= 1 {
        return samples;
    }

    samples
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
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
