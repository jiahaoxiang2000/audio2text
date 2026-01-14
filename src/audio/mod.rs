use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

const TARGET_SAMPLE_RATE: u32 = 16000;
const TARGET_CHANNELS: u16 = 1;
const CHUNK_DURATION_MS: u32 = 100;

pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    is_recording: Arc<AtomicBool>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            stream: None,
            is_recording: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&mut self, audio_tx: mpsc::Sender<Vec<u8>>) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("No input device available")?;

        info!("Using input device: {}", device.name()?);

        let supported_configs = device.supported_input_configs()?;
        debug!("Supported configs:");
        for config in supported_configs {
            debug!("  {:?}", config);
        }

        // Try to find a config that matches our target
        let config = find_best_config(&device)?;
        info!("Using config: {:?}", config);

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();
        let sample_format = config.sample_format();

        self.is_recording.store(true, Ordering::SeqCst);
        let is_recording = self.is_recording.clone();

        // Calculate samples per chunk based on the actual sample rate
        let samples_per_chunk = (sample_rate * CHUNK_DURATION_MS / 1000) as usize;

        let stream = match sample_format {
            SampleFormat::I8 => build_stream_i8(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::I16 => build_stream_i16(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::I32 => build_stream_i32(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::I64 => build_stream_i64(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::U8 => build_stream_u8(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::U16 => build_stream_u16(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::U32 => build_stream_u32(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::U64 => build_stream_u64(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::F32 => build_stream_f32(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            SampleFormat::F64 => build_stream_f64(
                &device,
                &config.into(),
                audio_tx,
                is_recording,
                sample_rate,
                channels,
                samples_per_chunk,
            )?,
            _ => return Err(anyhow::anyhow!("Unsupported sample format: {:?}", sample_format)),
        };

        stream.play()?;
        self.stream = Some(stream);

        info!("Audio capture started");
        Ok(())
    }

    pub fn stop(&mut self) {
        self.is_recording.store(false, Ordering::SeqCst);
        self.stream = None;
        info!("Audio capture stopped");
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }
}

fn find_best_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig> {
    let supported_configs: Vec<_> = device.supported_input_configs()?.collect();

    // First, try to find exact match
    for config in &supported_configs {
        if config.channels() == TARGET_CHANNELS
            && config.min_sample_rate().0 <= TARGET_SAMPLE_RATE
            && config.max_sample_rate().0 >= TARGET_SAMPLE_RATE
        {
            return Ok(config.clone().with_sample_rate(SampleRate(TARGET_SAMPLE_RATE)));
        }
    }

    // Try mono with any sample rate
    for config in &supported_configs {
        if config.channels() == TARGET_CHANNELS {
            let sample_rate = if config.min_sample_rate().0 <= TARGET_SAMPLE_RATE
                && config.max_sample_rate().0 >= TARGET_SAMPLE_RATE
            {
                TARGET_SAMPLE_RATE
            } else {
                config.max_sample_rate().0.min(48000)
            };
            return Ok(config.clone().with_sample_rate(SampleRate(sample_rate)));
        }
    }

    // Fall back to stereo
    for config in &supported_configs {
        if config.channels() == 2 {
            let sample_rate = if config.min_sample_rate().0 <= TARGET_SAMPLE_RATE
                && config.max_sample_rate().0 >= TARGET_SAMPLE_RATE
            {
                TARGET_SAMPLE_RATE
            } else {
                config.max_sample_rate().0.min(48000)
            };
            return Ok(config.clone().with_sample_rate(SampleRate(sample_rate)));
        }
    }

    // Use default
    device
        .default_input_config()
        .context("No suitable input config found")
}

// Helper function to process audio data regardless of type
fn process_audio_samples(
    samples_f32: Vec<f32>,
    source_channels: u16,
    buffer: &mut Vec<f32>,
    samples_per_chunk: usize,
    resampler: &Option<SimpleResampler>,
    audio_tx: &mpsc::Sender<Vec<u8>>,
) {
    // Convert to mono if stereo
    let mono_samples: Vec<f32> = if source_channels == 2 {
        samples_f32
            .chunks(2)
            .map(|chunk| (chunk[0] + chunk.get(1).copied().unwrap_or(0.0)) / 2.0)
            .collect()
    } else if source_channels > 2 {
        samples_f32
            .chunks(source_channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / source_channels as f32)
            .collect()
    } else {
        samples_f32
    };

    buffer.extend(mono_samples);

    // Process complete chunks
    while buffer.len() >= samples_per_chunk {
        let chunk: Vec<f32> = buffer.drain(..samples_per_chunk).collect();

        // Resample if necessary
        let resampled = if let Some(ref resampler) = resampler {
            resampler.resample(&chunk)
        } else {
            chunk
        };

        // Convert to i16 PCM bytes
        let pcm_bytes: Vec<u8> = resampled
            .iter()
            .flat_map(|&sample| {
                let clamped = sample.clamp(-1.0, 1.0);
                let i16_sample = (clamped * 32767.0) as i16;
                i16_sample.to_le_bytes()
            })
            .collect();

        // Send to channel (non-blocking)
        if let Err(e) = audio_tx.try_send(pcm_bytes) {
            warn!("Failed to send audio chunk: {}", e);
        }
    }
}

fn build_stream_i8(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[i8], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert i8 to f32 (range -128 to 127)
            let samples: Vec<f32> = data.iter().map(|&s| s as f32 / 128.0).collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_i16(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[i16], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert i16 to f32 (range -32768 to 32767)
            let samples: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_i32(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[i32], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert i32 to f32
            let samples: Vec<f32> = data
                .iter()
                .map(|&s| s as f32 / (i32::MAX as f32))
                .collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_i64(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[i64], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert i64 to f32
            let samples: Vec<f32> = data
                .iter()
                .map(|&s| (s as f64 / i64::MAX as f64) as f32)
                .collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_u8(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[u8], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert u8 to f32 (range 0 to 255, center at 128)
            let samples: Vec<f32> = data
                .iter()
                .map(|&s| (s as f32 - 128.0) / 128.0)
                .collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_u16(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[u16], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert u16 to f32 (range 0 to 65535, center at 32768)
            let samples: Vec<f32> = data
                .iter()
                .map(|&s| (s as f32 - 32768.0) / 32768.0)
                .collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_u32(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[u32], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert u32 to f32
            let samples: Vec<f32> = data
                .iter()
                .map(|&s| ((s as f64 - (u32::MAX as f64 / 2.0)) / (u32::MAX as f64 / 2.0)) as f32)
                .collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_u64(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[u64], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert u64 to f32
            let samples: Vec<f32> = data
                .iter()
                .map(|&s| {
                    ((s as f64 - (u64::MAX as f64 / 2.0)) / (u64::MAX as f64 / 2.0)) as f32
                })
                .collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_f32(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // f32 samples are already in the correct range (-1.0 to 1.0)
            let samples: Vec<f32> = data.to_vec();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn build_stream_f64(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: mpsc::Sender<Vec<u8>>,
    is_recording: Arc<AtomicBool>,
    source_sample_rate: u32,
    source_channels: u16,
    samples_per_chunk: usize,
) -> Result<cpal::Stream> {
    let mut buffer: Vec<f32> = Vec::with_capacity(samples_per_chunk * source_channels as usize * 2);
    let resampler = if source_sample_rate != TARGET_SAMPLE_RATE {
        Some(SimpleResampler::new(source_sample_rate, TARGET_SAMPLE_RATE))
    } else {
        None
    };

    let err_fn = |err| error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[f64], _: &cpal::InputCallbackInfo| {
            if !is_recording.load(Ordering::SeqCst) {
                return;
            }

            // Convert f64 to f32
            let samples: Vec<f32> = data.iter().map(|&s| s as f32).collect();

            process_audio_samples(
                samples,
                source_channels,
                &mut buffer,
                samples_per_chunk,
                &resampler,
                &audio_tx,
            );
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

struct SimpleResampler {
    source_rate: u32,
    target_rate: u32,
}

impl SimpleResampler {
    fn new(source_rate: u32, target_rate: u32) -> Self {
        Self {
            source_rate,
            target_rate,
        }
    }

    fn resample(&self, input: &[f32]) -> Vec<f32> {
        if self.source_rate == self.target_rate {
            return input.to_vec();
        }

        let ratio = self.target_rate as f64 / self.source_rate as f64;
        let output_len = (input.len() as f64 * ratio) as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_pos = i as f64 / ratio;
            let src_idx = src_pos as usize;
            let frac = src_pos - src_idx as f64;

            let sample = if src_idx + 1 < input.len() {
                // Linear interpolation
                input[src_idx] * (1.0 - frac as f32) + input[src_idx + 1] * frac as f32
            } else if src_idx < input.len() {
                input[src_idx]
            } else {
                0.0
            };

            output.push(sample);
        }

        output
    }
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self::new()
    }
}
