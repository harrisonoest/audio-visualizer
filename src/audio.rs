use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SampleFormat, Stream, StreamConfig};
use ringbuf::{
    HeapRb,
    traits::{Consumer, Observer, Producer, Split},
};
use rustfft::{FftPlanner, num_complex::Complex};
use tokio::sync::mpsc;

/// Audio capture and processing module
pub struct AudioProcessor {
    _stream: Stream,
    fft_rx: mpsc::Receiver<Vec<f32>>,
    sample_rate: u32,
}

impl AudioProcessor {
    /// Get the best available audio host, preferring non-ALSA backends to avoid timestamp issues
    fn get_best_audio_host() -> cpal::Host {
        // Get all available hosts and try them in order
        let available_hosts = cpal::available_hosts();

        // Try non-default hosts first (typically more stable than ALSA)
        for host_id in available_hosts {
            if let Ok(host) = cpal::host_from_id(host_id) {
                // Test if the host actually works by checking for devices
                if let Ok(mut devices) = host.input_devices() {
                    if devices.next().is_some() {
                        // This host has input devices, use it
                        return host;
                    }
                }
            }
        }

        // Fallback to default host
        cpal::default_host()
    }

    /// Create a new AudioProcessor with the specified device
    pub fn new(device: Option<Device>) -> Result<Self> {
        // Try different hosts in order of preference to avoid ALSA timestamp issues
        let host = Self::get_best_audio_host();
        let device = match device {
            Some(dev) => dev,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No input device available"))?,
        };

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        // Create a ring buffer for audio samples
        let buffer_size = sample_rate as usize; // 1 second of audio
        let rb = HeapRb::<f32>::new(buffer_size);
        let (producer, mut consumer) = rb.split();

        // Create channel for FFT results
        let (fft_tx, fft_rx) = mpsc::channel(10);

        // Clone for move into stream closure
        let fft_tx_clone = fft_tx.clone();

        // Build the input stream with error handling
        let stream = match config.sample_format() {
            SampleFormat::F32 => {
                Self::build_stream::<f32>(&device, &config.into(), producer, channels)
            }
            SampleFormat::I16 => {
                Self::build_stream::<i16>(&device, &config.into(), producer, channels)
            }
            SampleFormat::U16 => {
                Self::build_stream::<u16>(&device, &config.into(), producer, channels)
            }
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        let stream = stream?;

        // Start the stream
        stream.play()?;

        // Spawn FFT processing task
        tokio::spawn(async move {
            let mut fft_planner = FftPlanner::new();
            let fft_size = 1024;
            let fft = fft_planner.plan_fft_forward(fft_size);
            let mut buffer = vec![Complex::new(0.0, 0.0); fft_size];
            let mut samples = vec![0.0f32; fft_size];

            loop {
                // Collect samples from ring buffer
                if consumer.occupied_len() >= fft_size {
                    for sample in samples.iter_mut().take(fft_size) {
                        *sample = consumer.try_pop().unwrap_or(0.0);
                    }

                    // Apply window function (Hann window)
                    for (i, sample) in samples.iter_mut().enumerate() {
                        let window = 0.5
                            * (1.0
                                - ((2.0 * std::f32::consts::PI * i as f32)
                                    / (fft_size - 1) as f32)
                                    .cos());
                        *sample *= window;
                        buffer[i] = Complex::new(*sample, 0.0);
                    }

                    // Perform FFT
                    fft.process(&mut buffer);

                    // Calculate magnitude spectrum (only first half due to symmetry)
                    let magnitudes: Vec<f32> =
                        buffer.iter().take(fft_size / 2).map(|c| c.norm()).collect();

                    // Send results
                    if fft_tx_clone.send(magnitudes).await.is_err() {
                        break; // Receiver dropped
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(16)).await; // ~60 FPS
            }
        });

        Ok(AudioProcessor {
            _stream: stream,
            fft_rx,
            sample_rate,
        })
    }

    /// Build audio stream for a specific sample type with ALSA-safe configuration
    fn build_stream<T>(
        device: &Device,
        config: &StreamConfig,
        mut producer: ringbuf::HeapProd<f32>,
        channels: u16,
    ) -> Result<Stream>
    where
        T: Sample + Into<f32> + cpal::SizedSample,
    {
        // Use a configuration that's less likely to cause ALSA timestamp issues
        let mut safe_config = config.clone();

        // Use default buffer size which is usually safer
        safe_config.buffer_size = cpal::BufferSize::Default;

        let result = device.build_input_stream(
            &safe_config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // Convert samples to f32 and handle multi-channel by averaging
                for chunk in data.chunks(channels as usize) {
                    let sample = chunk.iter().map(|&s| s.into()).sum::<f32>() / channels as f32;

                    let _ = producer.try_push(sample);
                }
            },
            |err| {
                // Suppress common ALSA errors that are mostly harmless
                let err_str = err.to_string();
                if !err_str.contains("htstamp")
                    && !err_str.contains("timestamp")
                    && !err_str.contains("trigger")
                    && !err_str.contains("spuriously returned")
                    && !err_str.contains("poll()")
                {
                    eprintln!("Audio stream error: {err}");
                }
            },
            None, // No timeout to avoid timestamp checking
        );

        result.map_err(|e| anyhow::anyhow!("Failed to create audio stream: {e}"))
    }

    /// Get the latest FFT data
    pub async fn get_fft_data(&mut self) -> Option<Vec<f32>> {
        // Get the most recent FFT data, discarding older ones
        let mut latest = None;
        while let Ok(data) = self.fft_rx.try_recv() {
            latest = Some(data);
        }
        latest
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Get available audio input devices
pub fn get_input_devices() -> Result<Vec<(String, Device)>> {
    let host = AudioProcessor::get_best_audio_host();
    let mut devices = Vec::new();

    for device in host.input_devices()? {
        if let Ok(name) = device.name() {
            devices.push((name, device));
        }
    }

    Ok(devices)
}
