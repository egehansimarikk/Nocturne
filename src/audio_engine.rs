use biquad::{Biquad, Coefficients, DirectForm2Transposed, ToHertz, Type, Q_BUTTERWORTH_F32};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wasapi::{
    get_default_device, AudioClient, Device, DeviceCollection, Direction, SampleType, ShareMode,
    WaveFormat,
};

const BAND_CENTERS: [f32; 10] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// pcm_ring'in tutabileceği en fazla ses saniyesi.
const MAX_RING_SECONDS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputShareMode {
    Shared = 0,
    Exclusive = 1,
}

#[derive(Debug, Clone)]
pub struct AudioOutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone)]
pub struct AudioDiagnostics {
    pub badge: String,
    pub active_device_name: String,
    pub active_mode: OutputShareMode,
    pub exclusive_verified: bool,
    pub sample_rate: usize,
    pub bit_depth: usize,
    pub channels: usize,
    pub eq_active: bool,
    pub status_detail: String,
}

impl Default for AudioDiagnostics {
    fn default() -> Self {
        Self {
            badge: "48 kHz / 24-bit • WASAPI Shared".to_string(),
            active_device_name: "Varsayılan Çıkış Aygıtı".to_string(),
            active_mode: OutputShareMode::Shared,
            exclusive_verified: false,
            sample_rate: 48000,
            bit_depth: 24,
            channels: 2,
            eq_active: false,
            status_detail: "Ses motoru hazırlanıyor...".to_string(),
        }
    }
}

#[derive(Clone)]
struct EngineShared {
    mode: Arc<AtomicU8>,
    eq_enabled: Arc<AtomicBool>,
    bands: Arc<Mutex<[f32; 10]>>,
    preamp_db: Arc<AtomicU32>,
    selected_device_id: Arc<Mutex<Option<String>>>,
    restart_flag: Arc<AtomicBool>,
    web_eq_applied: Arc<AtomicBool>,
    diagnostics: Arc<Mutex<AudioDiagnostics>>,
}

pub struct AudioEngine {
    shared: EngineShared,
}

impl AudioEngine {
    pub fn start(
        initial_mode: OutputShareMode,
        initial_eq_enabled: bool,
        initial_bands: [f32; 10],
        initial_preamp_db: f32,
        initial_device_id: Option<String>,
    ) -> Self {
        let shared = EngineShared {
            mode: Arc::new(AtomicU8::new(initial_mode as u8)),
            eq_enabled: Arc::new(AtomicBool::new(initial_eq_enabled)),
            bands: Arc::new(Mutex::new(initial_bands)),
            preamp_db: Arc::new(AtomicU32::new(initial_preamp_db.to_bits())),
            selected_device_id: Arc::new(Mutex::new(initial_device_id)),
            restart_flag: Arc::new(AtomicBool::new(false)),
            web_eq_applied: Arc::new(AtomicBool::new(false)),
            diagnostics: Arc::new(Mutex::new(AudioDiagnostics::default())),
        };

        let worker_shared = shared.clone();
        thread::spawn(move || run_audio_worker(worker_shared));

        Self { shared }
    }

    pub fn enumerate_output_devices() -> Vec<AudioOutputDevice> {
        thread::spawn(|| {
            std::panic::catch_unwind(|| {
                let _ = wasapi::initialize_mta();
                let mut result = Vec::new();
                let default_id = default_device_id();

                if let Ok(collection) = DeviceCollection::new(&Direction::Render)
                    && let Ok(count) = collection.get_nbr_devices()
                {
                    for i in 0..count {
                        if let Ok(dev) = collection.get_device_at_index(i) {
                            let id = dev.get_id().unwrap_or_default();
                            let name = dev
                                .get_friendlyname()
                                .unwrap_or_else(|_| format!("Ses Aygıtı #{}", i + 1));
                            if !id.is_empty() {
                                result.push(AudioOutputDevice {
                                    is_default: !default_id.is_empty() && id == default_id,
                                    id,
                                    name,
                                });
                            }
                        }
                    }
                }
                result
            })
            .unwrap_or_default()
        })
        .join()
        .unwrap_or_default()
    }

    pub fn set_output_device(&self, device_id: Option<String>) {
        if let Ok(mut guard) = self.shared.selected_device_id.lock() {
            *guard = device_id;
        }
        self.shared.restart_flag.store(true, Ordering::Relaxed);
    }

    pub fn set_mode(&self, next: OutputShareMode) {
        self.shared.mode.store(next as u8, Ordering::Relaxed);
        self.shared.restart_flag.store(true, Ordering::Relaxed);
    }

    pub fn set_eq(&self, enabled: bool, new_bands: [f32; 10], preamp_db: f32) {
        self.shared.eq_enabled.store(enabled, Ordering::Relaxed);
        if let Ok(mut guard) = self.shared.bands.lock() {
            *guard = new_bands;
        }
        self.shared
            .preamp_db
            .store(preamp_db.clamp(-12.0, 6.0).to_bits(), Ordering::Relaxed);

        let verified = self.shared.web_eq_applied.load(Ordering::Relaxed);
        if let Ok(mut diagnostics) = self.shared.diagnostics.lock() {
            diagnostics.eq_active = match diagnostics.active_mode {
                OutputShareMode::Shared => enabled && verified,
                OutputShareMode::Exclusive => enabled,
            };
        }
    }

    pub fn set_web_eq_applied(&self, applied: bool) {
        self.shared
            .web_eq_applied
            .store(applied, Ordering::Relaxed);
        let eq_on = self.shared.eq_enabled.load(Ordering::Relaxed);
        if let Ok(mut diagnostics) = self.shared.diagnostics.lock() {
            diagnostics.eq_active = match diagnostics.active_mode {
                OutputShareMode::Shared => eq_on && applied,
                OutputShareMode::Exclusive => eq_on,
            };
        }
    }

    pub fn notify_playback_started(&self) {}

    pub fn get_diagnostics(&self) -> AudioDiagnostics {
        self.shared
            .diagnostics
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn format_badge(rate: usize, bits: usize, mode: OutputShareMode) -> String {
        let khz = if rate.is_multiple_of(1000) {
            format!("{} kHz", rate / 1000)
        } else {
            format!("{:.1} kHz", rate as f32 / 1000.0)
        };
        let mode_str = match mode {
            OutputShareMode::Shared => "WASAPI Shared",
            OutputShareMode::Exclusive => "WASAPI Exclusive",
        };
        format!("{khz} / {bits}-bit • {mode_str}")
    }
}

fn default_device_id() -> String {
    get_default_device(&Direction::Render)
        .ok()
        .and_then(|device| device.get_id().ok())
        .unwrap_or_default()
}

fn find_target_device(selected_id: &Option<String>) -> anyhow::Result<Device> {
    if let Some(target_id) = selected_id
        && !target_id.is_empty()
        && let Ok(collection) = DeviceCollection::new(&Direction::Render)
        && let Ok(count) = collection.get_nbr_devices()
    {
        for i in 0..count {
            if let Ok(dev) = collection.get_device_at_index(i)
                && let Ok(id) = dev.get_id()
                && &id == target_id
            {
                return Ok(dev);
            }
        }
    }
    Ok(get_default_device(&Direction::Render)?)
}

fn build_filters(sample_rate: f32, gains: &[f32; 10]) -> [DirectForm2Transposed<f32>; 10] {
    std::array::from_fn(|i| {
        let freq = BAND_CENTERS[i].min(sample_rate * 0.45);
        let filter_type = if i == 0 {
            Type::LowShelf(gains[i])
        } else if i == 9 {
            Type::HighShelf(gains[i])
        } else {
            Type::PeakingEQ(gains[i])
        };

        let coeffs = Coefficients::<f32>::from_params(
            filter_type,
            sample_rate.hz(),
            freq.hz(),
            Q_BUTTERWORTH_F32,
        )
        .unwrap_or_else(|_| {
            Coefficients::<f32>::from_params(
                Type::PeakingEQ(0.0),
                sample_rate.hz(),
                1000.0.hz(),
                Q_BUTTERWORTH_F32,
            )
            .unwrap()
        });
        DirectForm2Transposed::<f32>::new(coeffs)
    })
}

fn eq_preamp(gains: &[f32; 10], preamp_db: f32) -> f32 {
    let max_boost_db = gains.iter().copied().fold(0.0_f32, f32::max);
    10.0_f32.powf((preamp_db - max_boost_db) / 20.0)
}

fn soft_limit(sample: f32) -> f32 {
    const THRESHOLD: f32 = 0.98;
    const HEADROOM: f32 = 1.0 - THRESHOLD;

    if !sample.is_finite() {
        return 0.0;
    }

    let magnitude = sample.abs();
    if magnitude <= THRESHOLD {
        sample.clamp(-1.0, 1.0)
    } else {
        let over = magnitude - THRESHOLD;
        (THRESHOLD + HEADROOM * over / (over + HEADROOM)).copysign(sample)
    }
}

fn apply_fade_in(samples: &mut [f32], channels: usize, ramp_frames: usize) {
    let channels = channels.max(1);
    let ramp_frames = ramp_frames.min(samples.len() / channels);
    for (frame, chunk) in samples.chunks_mut(channels).enumerate().take(ramp_frames) {
        let gain = frame as f32 / ramp_frames as f32;
        for sample in chunk {
            *sample *= gain;
        }
    }
}

fn channel_mask_for(channels: usize) -> Option<u32> {
    match channels {
        1 => Some(0x4),
        2 => Some(0x3),
        6 => Some(0x3F),
        8 => Some(0x63F),
        _ => None,
    }
}

fn run_audio_worker(shared: EngineShared) {
    let _ = wasapi::initialize_mta();
    let mut failures: u32 = 0;

    loop {
        shared.restart_flag.store(false, Ordering::Relaxed);
        let target_mode = if shared.mode.load(Ordering::Relaxed) == OutputShareMode::Exclusive as u8
        {
            OutputShareMode::Exclusive
        } else {
            OutputShareMode::Shared
        };

        let outcome = match target_mode {
            OutputShareMode::Shared => run_shared_monitor(&shared),
            OutputShareMode::Exclusive => run_exclusive_bridge(&shared),
        };

        let Err(err) = outcome else {
            failures = 0;
            continue;
        };

        failures = failures.saturating_add(1);
        if let Ok(mut diagnostics) = shared.diagnostics.lock() {
            diagnostics.exclusive_verified = false;
            diagnostics.active_mode = OutputShareMode::Shared;
            diagnostics.eq_active = false;
            diagnostics.badge = AudioEngine::format_badge(
                diagnostics.sample_rate,
                diagnostics.bit_depth,
                OutputShareMode::Shared,
            );
            diagnostics.status_detail = if target_mode == OutputShareMode::Exclusive {
                format!(
                    "Exclusive akış kullanılamıyor ({err}) — Shared moda geçildi, yeniden bağlanılıyor (deneme {failures})"
                )
            } else {
                format!(
                    "Çıkış akışı koptu ({err}) — ses aygıtı yeniden aranıyor (deneme {failures})"
                )
            };
        }

        if target_mode == OutputShareMode::Exclusive {
            shared
                .mode
                .store(OutputShareMode::Shared as u8, Ordering::Relaxed);
        }

        let backoff_ms = 250 * u64::from(failures.min(8));
        thread::sleep(Duration::from_millis(backoff_ms));
    }
}

fn shared_status_text(
    rate: usize,
    bits: usize,
    channels: usize,
    eq_on: bool,
    eq_verified: bool,
) -> String {
    let eq = if !eq_on {
        "EQ kapalı (bypass)"
    } else if eq_verified {
        "EQ WebView2 ses zincirinde doğrulandı"
    } else {
        "EQ WebView2 ses zincirine bağlanmayı bekliyor"
    };
    format!("AKTİF: WASAPI Shared • {rate} Hz / {bits}-bit • {channels} Kanal • {eq}")
}

fn run_shared_monitor(shared: &EngineShared) -> anyhow::Result<()> {
    let device = get_default_device(&Direction::Render)?;
    let device_id = device.get_id().unwrap_or_default();
    let device_name = device
        .get_friendlyname()
        .unwrap_or_else(|_| "Varsayılan Çıkış Aygıtı".to_string());

    let client = device.get_iaudioclient()?;
    let mix_format = client.get_mixformat()?;
    let rate = mix_format.get_samplespersec() as usize;
    let bits = mix_format.get_bitspersample() as usize;
    let channels = mix_format.get_nchannels() as usize;

    let eq_on = shared.eq_enabled.load(Ordering::Relaxed);
    let eq_verified = shared.web_eq_applied.load(Ordering::Relaxed);

    if let Ok(mut diagnostics) = shared.diagnostics.lock() {
        diagnostics.badge = AudioEngine::format_badge(rate, bits, OutputShareMode::Shared);
        diagnostics.active_device_name = device_name;
        diagnostics.active_mode = OutputShareMode::Shared;
        diagnostics.exclusive_verified = false;
        diagnostics.sample_rate = rate;
        diagnostics.bit_depth = bits;
        diagnostics.channels = channels;
        diagnostics.eq_active = eq_on && eq_verified;
        diagnostics.status_detail =
            shared_status_text(rate, bits, channels, eq_on, eq_verified);
    }

    let mut last_device_check = Instant::now();
    let mut last_status = Instant::now();

    while !shared.restart_flag.load(Ordering::Relaxed) {
        let eq_on = shared.eq_enabled.load(Ordering::Relaxed);
        let eq_verified = shared.web_eq_applied.load(Ordering::Relaxed);

        if let Ok(mut diagnostics) = shared.diagnostics.lock() {
            diagnostics.eq_active = eq_on && eq_verified;
        }

        if last_status.elapsed() >= Duration::from_secs(1) {
            last_status = Instant::now();
            if let Ok(mut diagnostics) = shared.diagnostics.lock() {
                diagnostics.status_detail =
                    shared_status_text(rate, bits, channels, eq_on, eq_verified);
            }
        }

        if last_device_check.elapsed() >= Duration::from_secs(2) {
            last_device_check = Instant::now();
            let current_id = default_device_id();
            if !device_id.is_empty() && !current_id.is_empty() && current_id != device_id {
                return Err(anyhow::anyhow!("varsayılan çıkış aygıtı değişti"));
            }
        }

        thread::sleep(Duration::from_millis(200));
    }

    Ok(())
}

fn run_exclusive_bridge(shared: &EngineShared) -> anyhow::Result<()> {
    let selected_device_id = shared
        .selected_device_id
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let device = find_target_device(&selected_device_id)?;
    let device_name = device
        .get_friendlyname()
        .unwrap_or_else(|_| "Seçili Ses Aygıtı".to_string());

    let mut render_client = device.get_iaudioclient()?;
    let mix_format = render_client.get_mixformat()?;
    let rate = mix_format.get_samplespersec() as usize;
    let channels = mix_format.get_nchannels() as usize;
    let channel_count = channels.max(1);
    let mask = channel_mask_for(channels);
    let (_, min_time) = render_client.get_periods()?;

    let fmt24 = WaveFormat::new(24, 32, &SampleType::Int, rate, channels, mask);
    let fmt16 = WaveFormat::new(16, 16, &SampleType::Int, rate, channels, mask);

    let (out_format, bits) = if render_client
        .is_supported(&fmt24, &ShareMode::Exclusive)
        .is_ok()
    {
        (fmt24, 24)
    } else if render_client
        .is_supported(&fmt16, &ShareMode::Exclusive)
        .is_ok()
    {
        (fmt16, 16)
    } else {
        anyhow::bail!(
            "Donanım {} Hz / {} kanal için Exclusive PCM desteklemiyor",
            rate,
            channels
        );
    };

    render_client
        .initialize_client(
            &out_format,
            min_time,
            &Direction::Render,
            &ShareMode::Exclusive,
            false,
        )
        .map_err(|e| anyhow::anyhow!("Donanım kilidi reddedildi: {e}"))?;

    let _render_event = render_client
        .set_get_eventhandle()
        .map_err(|e| anyhow::anyhow!("Render EventHandle hatası: {e}"))?;

    let render_buf_frames = render_client.get_bufferframecount()?;
    let audio_render = render_client.get_audiorenderclient()?;

    let pid = std::process::id();
    let cap_format = WaveFormat::new(32, 32, &SampleType::Float, rate, channels, mask);
    let mut capture_client = AudioClient::new_application_loopback_client(pid, true)
        .map_err(|e| anyhow::anyhow!("Process Loopback oluşturulamadı: {e}"))?;

    capture_client
        .initialize_client(
            &cap_format,
            100_000,
            &Direction::Capture,
            &ShareMode::Shared,
            true,
        )
        .map_err(|e| anyhow::anyhow!("Process Loopback init hatası: {e}"))?;

    let cap_event = capture_client
        .set_get_eventhandle()
        .map_err(|e| anyhow::anyhow!("Loopback EventHandle hatası: {e}"))?;

    let audio_capture = capture_client
        .get_audiocaptureclient()
        .map_err(|e| anyhow::anyhow!("CaptureClient alınamadı: {e}"))?;

    if let Ok(mut diagnostics) = shared.diagnostics.lock() {
        diagnostics.badge = AudioEngine::format_badge(rate, bits, OutputShareMode::Exclusive);
        diagnostics.active_device_name = device_name;
        diagnostics.active_mode = OutputShareMode::Exclusive;
        diagnostics.exclusive_verified = true;
        diagnostics.sample_rate = rate;
        diagnostics.bit_depth = bits;
        diagnostics.channels = channels;
        diagnostics.eq_active = shared.eq_enabled.load(Ordering::Relaxed);
        diagnostics.status_detail = format!(
            "DOĞRULANDI: Donanım kilidi aktif ({} Hz / {}-bit PCM, min periyot {}µs)",
            rate,
            bits,
            min_time / 10
        );
    }

    let mut last_gains = shared.bands.lock().map(|g| *g).unwrap_or([0.0; 10]);
    let mut last_preamp_db = f32::from_bits(shared.preamp_db.load(Ordering::Relaxed));
    let mut preamp = eq_preamp(&last_gains, last_preamp_db);
    let mut filters_by_channel = (0..channel_count)
        .map(|_| build_filters(rate as f32, &last_gains))
        .collect::<Vec<_>>();

    capture_client
        .start_stream()
        .map_err(|e| anyhow::anyhow!("Loopback akış başlatılamadı: {e}"))?;
    render_client
        .start_stream()
        .map_err(|e| anyhow::anyhow!("Exclusive çıkış başlatılamadı: {e}"))?;

    let max_ring_samples = rate * channel_count * MAX_RING_SECONDS;
    let mut cap_deque: VecDeque<u8> = VecDeque::with_capacity(rate * channel_count * 4);
    let mut pcm_ring: VecDeque<f32> = VecDeque::with_capacity(max_ring_samples);
    let ramp_frames = (rate / 200).max(1);
    let bytes_per_sample = if bits == 24 { 4usize } else { 2usize };

    let mut fade_pending = true;
    let mut capture_errors: u32 = 0;
    let mut underruns: u64 = 0;
    let mut overruns: u64 = 0;
    let mut last_stats = Instant::now();

    while !shared.restart_flag.load(Ordering::Relaxed) {
        match cap_event.wait_for_event(25) {
            Ok(()) => {}
            Err(wasapi::WasapiError::EventTimeout) => {
                continue;
            }
            Err(err) => {
                return Err(anyhow::anyhow!(
                    "loopback olayı alınamadı (aygıt koptu olabilir): {err}"
                ));
            }
        }

        let is_eq = shared.eq_enabled.load(Ordering::Relaxed);
        let current_preamp_db = f32::from_bits(shared.preamp_db.load(Ordering::Relaxed));
        if let Ok(gains) = shared.bands.lock() {
            let bands_changed = *gains != last_gains;
            if bands_changed || current_preamp_db != last_preamp_db {
                last_gains = *gains;
                last_preamp_db = current_preamp_db;
                preamp = eq_preamp(&last_gains, last_preamp_db);
                if bands_changed {
                    filters_by_channel = (0..channel_count)
                        .map(|_| build_filters(rate as f32, &last_gains))
                        .collect();
                    fade_pending = true;
                }
            }
        }

        match audio_capture.read_from_device_to_deque(&mut cap_deque) {
            Ok(_) => capture_errors = 0,
            Err(err) => {
                capture_errors += 1;
                if capture_errors >= 3 {
                    return Err(anyhow::anyhow!("loopback akışı okunamadı: {err}"));
                }
            }
        }

        let mut ch_idx = 0usize;
        while cap_deque.len() >= 4 {
            let b0 = cap_deque.pop_front().unwrap_or(0);
            let b1 = cap_deque.pop_front().unwrap_or(0);
            let b2 = cap_deque.pop_front().unwrap_or(0);
            let b3 = cap_deque.pop_front().unwrap_or(0);
            let mut sample = f32::from_le_bytes([b0, b1, b2, b3]);

            if is_eq {
                sample *= preamp;
                let ch = ch_idx % channel_count;
                if let Some(filters) = filters_by_channel.get_mut(ch) {
                    for f in filters {
                        sample = f.run(sample);
                    }
                }
            }

            ch_idx += 1;
            pcm_ring.push_back(soft_limit(sample));
        }

        let padding = render_client
            .get_current_padding()
            .map_err(|err| anyhow::anyhow!("çıkış tamponu durumu alınamadı: {err}"))?;
        let avail_frames = render_buf_frames.saturating_sub(padding) as usize;
        if avail_frames == 0 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        let needed_samples = avail_frames * channel_count;

        if pcm_ring.len() > max_ring_samples {
            let excess = pcm_ring.len() - max_ring_samples;
            pcm_ring.drain(..excess);
            overruns += 1;
            fade_pending = true;
        }

        let out_bytes: Vec<u8> = if pcm_ring.len() >= needed_samples {
            let mut chunk: Vec<f32> = pcm_ring.drain(..needed_samples).collect();
            if fade_pending {
                apply_fade_in(&mut chunk, channel_count, ramp_frames);
                fade_pending = false;
            }
            if bits == 24 {
                chunk
                    .iter()
                    .flat_map(|sample| {
                        let value = (sample.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
                        (value << 8).to_le_bytes()
                    })
                    .collect()
            } else {
                chunk
                    .iter()
                    .flat_map(|sample| {
                        ((sample.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes()
                    })
                    .collect()
            }
        } else {
            underruns += 1;
            fade_pending = true;
            vec![0u8; needed_samples * bytes_per_sample]
        };

        audio_render
            .write_to_device(avail_frames, &out_bytes, None)
            .map_err(|err| anyhow::anyhow!("çıkışa yazılamadı: {err}"))?;

        if last_stats.elapsed() >= Duration::from_secs(2) {
            last_stats = Instant::now();
            if let Ok(mut diagnostics) = shared.diagnostics.lock() {
                diagnostics.status_detail = format!(
                    "DOĞRULANDI: Donanım kilidi aktif ({rate} Hz / {bits}-bit PCM) • underrun {underruns} • overrun {overruns} • tampon {} ms",
                    (pcm_ring.len() / channel_count).saturating_mul(1000) / rate.max(1)
                );
            }
        }
    }

    let _ = capture_client.stop_stream();
    let _ = render_client.stop_stream();
    Ok(())
}