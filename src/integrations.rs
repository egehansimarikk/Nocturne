use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ============================================================================
// GÜVENLİ ANAHTAR YÖNETİMİ (Compile-time Env -> Yerel Override -> Gömülü Fallback)
// ============================================================================

const DISCORD_APP_ID: &str = "1573447652585119844";

const CREDENTIAL_XOR_KEY: [u8; 8] = [0x4E, 0x6F, 0x63, 0x74, 0x75, 0x72, 0x6E, 0x65]; // "Nocturne"

const MASKED_LASTFM_API_KEY: &[u8] = &[
    0x78, 0x0A, 0x01, 0x4C, 0x14, 0x10, 0x0C, 0x01, 0x7F, 0x58, 0x06, 0x45, 0x45, 0x42, 0x5B, 0x03,
    0x28, 0x5F, 0x56, 0x11, 0x45, 0x4B, 0x0A, 0x03, 0x2B, 0x5A, 0x06, 0x17, 0x13, 0x11, 0x5D, 0x06,
];

const MASKED_LASTFM_SHARED_SECRET: &[u8] = &[
    0x7D, 0x5B, 0x55, 0x10, 0x45, 0x11, 0x5F, 0x01, 0x7C, 0x5C, 0x05, 0x47, 0x17, 0x13, 0x5E, 0x57,
    0x7A, 0x5A, 0x06, 0x43, 0x14, 0x45, 0x0A, 0x04, 0x2A, 0x5C, 0x06, 0x4D, 0x10, 0x11, 0x08, 0x00,
];

#[derive(Debug, Clone)]
struct ResolvedCredentials {
    discord_app_id: String,
    lastfm_api_key: String,
    lastfm_shared_secret: String,
}

#[derive(Debug, Deserialize, Default)]
struct LocalCredentialsFile {
    #[serde(default)]
    discord_app_id: Option<String>,
    #[serde(default)]
    lastfm_api_key: Option<String>,
    #[serde(default)]
    lastfm_shared_secret: Option<String>,
}

fn deobfuscate_bytes(masked: &[u8]) -> String {
    let decoded: Vec<u8> = masked
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ CREDENTIAL_XOR_KEY[i % CREDENTIAL_XOR_KEY.len()])
        .collect();
    String::from_utf8(decoded).unwrap_or_default()
}

fn nocturne_data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("Nocturne");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn credentials_override_path() -> PathBuf {
    nocturne_data_dir().join("credentials.json")
}

fn load_local_credentials_override() -> LocalCredentialsFile {
    fs::read_to_string(credentials_override_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<LocalCredentialsFile>(&raw).ok())
        .unwrap_or_default()
}

fn resolved_credentials() -> &'static ResolvedCredentials {
    static CREDS: OnceLock<ResolvedCredentials> = OnceLock::new();
    CREDS.get_or_init(|| {
        let local = load_local_credentials_override();

        let discord_app_id = local
            .discord_app_id
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                option_env!("NOCTURNE_DISCORD_APP_ID")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| DISCORD_APP_ID.to_string());

        let lastfm_api_key = local
            .lastfm_api_key
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                option_env!("NOCTURNE_LASTFM_API_KEY")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| deobfuscate_bytes(MASKED_LASTFM_API_KEY));

        let lastfm_shared_secret = local
            .lastfm_shared_secret
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                option_env!("NOCTURNE_LASTFM_SHARED_SECRET")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| deobfuscate_bytes(MASKED_LASTFM_SHARED_SECRET));

        ResolvedCredentials {
            discord_app_id,
            lastfm_api_key,
            lastfm_shared_secret,
        }
    })
}

fn atomic_write_json(target: &PathBuf, json: &str) {
    let tmp = target.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        if fs::rename(&tmp, target).is_err() {
            let _ = fs::write(target, json);
            let _ = fs::remove_file(&tmp);
        }
    } else {
        let _ = fs::write(target, json);
    }
}

#[cfg(windows)]
fn dpapi_protect_string(plain: &str) -> Option<String> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    use windows::core::PCSTR;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    type CryptProtectDataFn = unsafe extern "system" fn(
        p_data_in: *const DataBlob,
        sz_data_descr: *const u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *mut c_void,
        p_prompt_struct: *mut c_void,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;

    if plain.is_empty() {
        return None;
    }

    unsafe {
        let module = LoadLibraryA(PCSTR(b"crypt32.dll\0".as_ptr())).ok()?;
        let proc = GetProcAddress(module, PCSTR(b"CryptProtectData\0".as_ptr()))?;
        let crypt_protect: CryptProtectDataFn = std::mem::transmute(proc);

        let in_blob = DataBlob {
            cb_data: plain.len() as u32,
            pb_data: plain.as_ptr() as *mut u8,
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };

        let ok = crypt_protect(
            &in_blob,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0x1,
            &mut out_blob,
        );
        if ok == 0 || out_blob.pb_data.is_null() {
            return None;
        }

        let slice = std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize);
        let mut hex = String::with_capacity(slice.len() * 2);
        for b in slice {
            hex.push_str(&format!("{:02x}", b));
        }
        let _ = LocalFree(Some(HLOCAL(out_blob.pb_data as *mut _)));
        Some(hex)
    }
}

#[cfg(windows)]
fn dpapi_unprotect_string(hex_cipher: &str) -> Option<String> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    use windows::core::PCSTR;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    type CryptUnprotectDataFn = unsafe extern "system" fn(
        p_data_in: *const DataBlob,
        ppsz_data_descr: *mut *mut u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *mut c_void,
        p_prompt_struct: *mut c_void,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;

    if hex_cipher.len() < 2 || hex_cipher.len() % 2 != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(hex_cipher.len() / 2);
    for chunk in hex_cipher.as_bytes().chunks_exact(2) {
        let s = std::str::from_utf8(chunk).ok()?;
        let val = u8::from_str_radix(s, 16).ok()?;
        bytes.push(val);
    }

    unsafe {
        let module = LoadLibraryA(PCSTR(b"crypt32.dll\0".as_ptr())).ok()?;
        let proc = GetProcAddress(module, PCSTR(b"CryptUnprotectData\0".as_ptr()))?;
        let crypt_unprotect: CryptUnprotectDataFn = std::mem::transmute(proc);

        let in_blob = DataBlob {
            cb_data: bytes.len() as u32,
            pb_data: bytes.as_mut_ptr(),
        };
        let mut out_blob = DataBlob {
            cb_data: 0,
            pb_data: std::ptr::null_mut(),
        };

        let ok = crypt_unprotect(
            &in_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0x1,
            &mut out_blob,
        );
        if ok == 0 || out_blob.pb_data.is_null() {
            return None;
        }

        let slice = std::slice::from_raw_parts(out_blob.pb_data, out_blob.cb_data as usize);
        let plain = String::from_utf8(slice.to_vec()).ok();
        let _ = LocalFree(Some(HLOCAL(out_blob.pb_data as *mut _)));
        plain
    }
}

// ============================================================================
// DISCORD RICH PRESENCE (Doğrudan Windows Named Pipe IPC)
// ============================================================================

struct DiscordRpcInner {
    enabled: bool,
    pipe: Option<std::fs::File>,
    last_track_id: u64,
    last_is_playing: Option<bool>,
    last_sent_at: Option<Instant>,
    last_connect_attempt: Option<Instant>,
}

#[derive(Clone)]
pub struct DiscordRpcManager {
    inner: Arc<Mutex<DiscordRpcInner>>,
}

impl Default for DiscordRpcManager {
    fn default() -> Self {
        Self::new(true)
    }
}

impl DiscordRpcManager {
    pub fn new(enabled: bool) -> Self {
        let mgr = Self {
            inner: Arc::new(Mutex::new(DiscordRpcInner {
                enabled,
                pipe: None,
                last_track_id: 0,
                last_is_playing: None,
                last_sent_at: None,
                last_connect_attempt: None,
            })),
        };
        if enabled {
            mgr.try_connect();
        }
        mgr
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut guard = self.inner.lock().unwrap();
        guard.enabled = enabled;
        if !enabled {
            if let Some(ref mut pipe) = guard.pipe {
                let _ = send_discord_clear(pipe);
            }
            guard.pipe = None;
            guard.last_track_id = 0;
            guard.last_is_playing = None;
        } else {
            drop(guard);
            self.try_connect();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.lock().unwrap().enabled
    }

    pub fn is_connected(&self) -> bool {
        let mut guard = self.inner.lock().unwrap();
        if !guard.enabled {
            return false;
        }
        if guard.pipe.is_some() {
            return true;
        }
        let can_retry = guard
            .last_connect_attempt
            .map(|t| t.elapsed() >= Duration::from_secs(4))
            .unwrap_or(true);
        if can_retry {
            guard.last_connect_attempt = Some(Instant::now());
            guard.pipe = connect_discord_ipc();
        }
        guard.pipe.is_some()
    }

    pub fn try_connect(&self) {
        let mut guard = self.inner.lock().unwrap();
        if !guard.enabled || guard.pipe.is_some() {
            return;
        }
        guard.last_connect_attempt = Some(Instant::now());
        guard.pipe = connect_discord_ipc();
    }

    pub fn on_playback_state(
        &self,
        track_id: u64,
        title: &str,
        artist: &str,
        artwork_url: &str,
        current_time: f64,
        duration: f64,
        is_playing: bool,
        pending_switch: bool,
    ) {
        if pending_switch || track_id == 0 || title.trim().is_empty() {
            return;
        }

        let mut guard = self.inner.lock().unwrap();
        if !guard.enabled {
            return;
        }

        let track_changed = guard.last_track_id != track_id;
        let play_state_changed = guard.last_is_playing != Some(is_playing);
        let periodic_refresh = guard
            .last_sent_at
            .map(|t| t.elapsed() >= Duration::from_secs(15))
            .unwrap_or(true);

        if !track_changed && !play_state_changed && !periodic_refresh {
            return;
        }

        if guard.pipe.is_none() {
            let can_retry = guard
                .last_connect_attempt
                .map(|t| t.elapsed() >= Duration::from_secs(5))
                .unwrap_or(true);
            if !can_retry {
                return;
            }
            guard.last_connect_attempt = Some(Instant::now());
            guard.pipe = connect_discord_ipc();
        }

        let Some(ref mut pipe) = guard.pipe else {
            return;
        };

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let timestamps = if is_playing && duration.is_finite() && duration > 0.0 {
            let cur = if current_time.is_finite() && current_time >= 0.0 {
                current_time
            } else {
                0.0
            };
            let start_ts = now_sec.saturating_sub(cur.round() as u64);
            let end_ts = start_ts + duration.round() as u64;
            serde_json::json!({
                "start": start_ts,
                "end": end_ts
            })
        } else {
            serde_json::Value::Null
        };

        let large_image =
            if artwork_url.starts_with("http://") || artwork_url.starts_with("https://") {
                artwork_url
            } else {
                "nocturne_logo"
            };

        let small_image = if is_playing { "play" } else { "pause" };
        let small_text = if is_playing {
            "Çalıyor • Deezer"
        } else {
            "Duraklatıldı"
        };

        let artist_line = if artist.trim().is_empty() {
            "Bilinmeyen Sanatçı".to_string()
        } else {
            format!("by {}", artist.trim())
        };

        let mut activity = serde_json::json!({
            "type": 2,
            "details": truncate_discord_field(title.trim(), 120),
            "state": truncate_discord_field(&artist_line, 120),
            "assets": {
                "large_image": large_image,
                "large_text": "Nocturne • Lossless Audio",
                "small_image": small_image,
                "small_text": small_text
            }
        });

        if !timestamps.is_null() {
            activity["timestamps"] = timestamps;
        }

        let payload = serde_json::json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": activity
            },
            "nonce": format!("nocturne-{}-{}", track_id, now_sec)
        });

        if write_discord_frame(pipe, 1, &payload.to_string()).is_ok() {
            if let Ok((_, resp)) = read_discord_frame(pipe) {
                if resp.contains("\"evt\":\"ERROR\"") {
                    guard.pipe = None;
                    guard.last_track_id = 0;
                    guard.last_is_playing = None;
                    return;
                }
            }
            guard.last_track_id = track_id;
            guard.last_is_playing = Some(is_playing);
            guard.last_sent_at = Some(Instant::now());
        } else {
            guard.pipe = None;
            guard.last_track_id = 0;
            guard.last_is_playing = None;
        }
    }

    pub fn clear(&self) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(ref mut pipe) = guard.pipe {
            let _ = send_discord_clear(pipe);
        }
        guard.last_track_id = 0;
        guard.last_is_playing = None;
    }
}

impl Drop for DiscordRpcManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            self.clear();
        }
    }
}

fn truncate_discord_field(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        if count < 2 {
            format!("{s} ")
        } else {
            s.to_string()
        }
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        out.push_str("...");
        out
    }
}

fn connect_discord_ipc() -> Option<std::fs::File> {
    let app_id = &resolved_credentials().discord_app_id;
    for i in 0..10 {
        let pipe_path = format!(r"\\?\pipe\discord-ipc-{i}");
        if let Ok(mut file) = OpenOptions::new().read(true).write(true).open(&pipe_path) {
            let handshake = serde_json::json!({
                "v": 1,
                "client_id": app_id
            });
            if write_discord_frame(&mut file, 0, &handshake.to_string()).is_ok() {
                if let Ok((op, resp)) = read_discord_frame(&mut file) {
                    if op == 1 && !resp.contains("\"evt\":\"ERROR\"") {
                        return Some(file);
                    }
                }
            }
        }
    }
    None
}

fn send_discord_clear(pipe: &mut std::fs::File) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "cmd": "SET_ACTIVITY",
        "args": {
            "pid": std::process::id()
        },
        "nonce": "nocturne-clear"
    });
    write_discord_frame(pipe, 1, &payload.to_string())?;
    let _ = read_discord_frame(pipe);
    Ok(())
}

fn write_discord_frame(pipe: &mut std::fs::File, opcode: u32, json: &str) -> std::io::Result<()> {
    let bytes = json.as_bytes();
    let len = bytes.len() as u32;
    let mut header = [0u8; 8];
    header[0..4].copy_from_slice(&opcode.to_le_bytes());
    header[4..8].copy_from_slice(&len.to_le_bytes());
    pipe.write_all(&header)?;
    pipe.write_all(bytes)?;
    pipe.flush()
}

fn read_discord_frame(pipe: &mut std::fs::File) -> std::io::Result<(u32, String)> {
    let mut header = [0u8; 8];
    pipe.read_exact(&mut header)?;
    let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if len > 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Frame too large",
        ));
    }
    let mut buf = vec![0u8; len];
    pipe.read_exact(&mut buf)?;
    Ok((opcode, String::from_utf8_lossy(&buf).into_owned()))
}

// ============================================================================
// LAST.FM SCROBBLING & OTURUM YÖNETİMİ (Hızlı Pre-fetch + Otomatik Bağlantı)
// ============================================================================

const LASTFM_API_URL: &str = "https://ws.audioscrobbler.com/2.0/";
const SCROBBLE_QUEUE_MAX_ITEMS: usize = 500;
const SCROBBLE_BATCH_SIZE: usize = 50;
const SCROBBLE_MAX_AGE_SEC: u64 = 14 * 24 * 60 * 60;
const PREFETCHED_TOKEN_TTL: Duration = Duration::from_secs(45 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LastFmSession {
    pub username: String,
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub encrypted_session_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingScrobbleItem {
    artist: String,
    track: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    album: Option<String>,
    duration: u64,
    timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastFmAuthStatus {
    LoggedOut,
    WaitingForBrowserApproval,
    LoggedIn { username: String },
    Error(String),
}

struct ScrobbleTracker {
    track_id: u64,
    title: String,
    artist: String,
    album: Option<String>,
    duration_sec: u64,
    started_unix: u64,
    accumulated_play_sec: f64,
    last_tick: Option<Instant>,
    last_now_playing_sent: Option<Instant>,
    scrobbled: bool,
}

impl ScrobbleTracker {
    fn new(
        track_id: u64,
        title: String,
        artist: String,
        album: Option<String>,
        duration_sec: u64,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            track_id,
            title,
            artist,
            album,
            duration_sec,
            started_unix: now,
            accumulated_play_sec: 0.0,
            last_tick: None,
            last_now_playing_sent: None,
            scrobbled: false,
        }
    }
}

struct LastFmInner {
    enabled: bool,
    session: Option<LastFmSession>,
    status: LastFmAuthStatus,
    prefetched_token: Option<(String, Instant)>,
    prefetch_in_flight: bool,
    tracker: Option<ScrobbleTracker>,
    offline_queue: VecDeque<PendingScrobbleItem>,
    flushing_queue: bool,
    last_flush_attempt: Option<Instant>,
    scrobbled_count: u64,
    last_scrobbled_track: Option<String>,
}

#[derive(Clone)]
pub struct LastFmManager {
    inner: Arc<Mutex<LastFmInner>>,
    client: reqwest::blocking::Client,
}

impl Default for LastFmManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LastFmManager {
    pub fn new() -> Self {
        let saved = load_lastfm_session_from_disk();
        let has_session = saved
            .as_ref()
            .map(|s| !s.session_key.is_empty())
            .unwrap_or(false);
        let status = match &saved {
            Some(s) if !s.session_key.is_empty() => LastFmAuthStatus::LoggedIn {
                username: s.username.clone(),
            },
            _ => LastFmAuthStatus::LoggedOut,
        };
        let offline_queue = load_scrobble_queue_from_disk();

        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(format!("Nocturne/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        let mgr = Self {
            inner: Arc::new(Mutex::new(LastFmInner {
                enabled: true,
                session: saved,
                status,
                prefetched_token: None,
                prefetch_in_flight: false,
                tracker: None,
                offline_queue,
                flushing_queue: false,
                last_flush_attempt: None,
                scrobbled_count: 0,
                last_scrobbled_track: None,
            })),
            client,
        };

        if has_session {
            mgr.spawn_flush_offline_queue();
        } else {
            // Oturum yoksa token + TLS bağlantısını önceden ısıt
            mgr.prefetch_auth_token();
        }
        mgr
    }

    pub fn prefetch_auth_token(&self) {
        let creds = resolved_credentials();
        if creds.lastfm_api_key.trim().is_empty() || creds.lastfm_shared_secret.trim().is_empty() {
            return;
        }

        {
            let mut guard = self.inner.lock().unwrap();
            if guard.session.is_some() || guard.prefetch_in_flight {
                return;
            }
            if let Some((_, fetched_at)) = &guard.prefetched_token
                && fetched_at.elapsed() < PREFETCHED_TOKEN_TTL
            {
                return;
            }
            guard.prefetch_in_flight = true;
        }

        let this = self.clone();
        thread::spawn(move || {
            let res = this.fetch_auth_token();
            let mut guard = this.inner.lock().unwrap();
            guard.prefetch_in_flight = false;
            if let Ok(token) = res {
                guard.prefetched_token = Some((token, Instant::now()));
            }
        });
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut guard = self.inner.lock().unwrap();
        guard.enabled = enabled;
        if !enabled {
            guard.tracker = None;
        } else {
            drop(guard);
            self.spawn_flush_offline_queue();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.lock().unwrap().enabled
    }

    pub fn status(&self) -> LastFmAuthStatus {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn username(&self) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .session
            .as_ref()
            .map(|s| s.username.clone())
    }

    pub fn pending_scrobble_count(&self) -> usize {
        self.inner.lock().unwrap().offline_queue.len()
    }

    pub fn scrobbled_count(&self) -> u64 {
        self.inner.lock().unwrap().scrobbled_count
    }

    pub fn last_scrobbled_track(&self) -> Option<String> {
        self.inner.lock().unwrap().last_scrobbled_track.clone()
    }

    pub fn status_message(&self) -> Option<String> {
        let guard = self.inner.lock().unwrap();
        match &guard.status {
            LastFmAuthStatus::WaitingForBrowserApproval => Some(
                "Tarayıcıda Last.fm erişimine izin ver; onayladığında otomatik bağlanır."
                    .to_string(),
            ),
            LastFmAuthStatus::Error(err) => Some(err.clone()),
            _ if !guard.offline_queue.is_empty() => Some(format!(
                "Çevrimdışı kuyrukta {} scrobble bekliyor; bağlantı sağlandığında gönderilecek.",
                guard.offline_queue.len()
            )),
            _ => None,
        }
    }

    pub fn start_auth(&self) {
        self.start_login();
    }

    /// Kullanıcının tarayıcısında Last.fm onay sayfasını anında açar ve arka planda otomatik bağlar.
    pub fn start_login(&self) {
        let creds = resolved_credentials();
        if creds.lastfm_api_key.trim().is_empty() || creds.lastfm_shared_secret.trim().is_empty() {
            self.inner.lock().unwrap().status = LastFmAuthStatus::Error(
                "Last.fm API anahtarları bulunamadı. NOCTURNE_LASTFM_API_KEY veya credentials.json ayarla."
                    .into(),
            );
            return;
        }

        let ready_token = {
            let mut guard = self.inner.lock().unwrap();
            if matches!(guard.status, LastFmAuthStatus::WaitingForBrowserApproval) {
                return;
            }
            guard.status = LastFmAuthStatus::WaitingForBrowserApproval;
            guard
                .prefetched_token
                .take()
                .filter(|(_, fetched_at)| fetched_at.elapsed() < PREFETCHED_TOKEN_TTL)
                .map(|(tok, _)| tok)
        };

        let this = self.clone();
        thread::spawn(move || {
            let token = match ready_token {
                Some(t) => t,
                None => match this.fetch_auth_token() {
                    Ok(t) => t,
                    Err(e) => {
                        this.inner.lock().unwrap().status = LastFmAuthStatus::Error(e);
                        return;
                    }
                },
            };

            let auth_url = format!(
                "https://www.last.fm/api/auth/?api_key={}&token={}",
                resolved_credentials().lastfm_api_key,
                token
            );
            if !open_url_in_default_browser(&auth_url) {
                this.inner.lock().unwrap().status = LastFmAuthStatus::Error(
                    "Tarayıcı açılamadı. Varsayılan tarayıcı ayarını kontrol edip tekrar dene."
                        .to_string(),
                );
                return;
            }

            // İlk 20 denemede 800 ms aralıkla hızlı kontrol, sonrasında 2 sn aralık
            for attempt in 0..55 {
                let wait = if attempt < 20 {
                    Duration::from_millis(800)
                } else {
                    Duration::from_secs(2)
                };
                thread::sleep(wait);

                if !matches!(
                    this.inner.lock().unwrap().status,
                    LastFmAuthStatus::WaitingForBrowserApproval
                ) {
                    return;
                }

                if let Ok(session) = this.exchange_token_for_session(&token) {
                    save_lastfm_session_to_disk(&session);
                    {
                        let mut guard = this.inner.lock().unwrap();
                        guard.status = LastFmAuthStatus::LoggedIn {
                            username: session.username.clone(),
                        };
                        guard.session = Some(session);
                    }
                    this.spawn_flush_offline_queue();
                    return;
                }
            }

            let mut guard = this.inner.lock().unwrap();
            if matches!(guard.status, LastFmAuthStatus::WaitingForBrowserApproval) {
                guard.status =
                    LastFmAuthStatus::Error("Giriş zaman aşımına uğradı, tekrar dene.".into());
            }
            drop(guard);
            this.prefetch_auth_token();
        });
    }

    pub fn logout(&self) {
        delete_lastfm_session_from_disk();
        {
            let mut guard = self.inner.lock().unwrap();
            guard.session = None;
            guard.tracker = None;
            guard.prefetched_token = None;
            guard.status = LastFmAuthStatus::LoggedOut;
        }
        self.prefetch_auth_token();
    }

    pub fn on_playback_state(
        &self,
        track_id: u64,
        title: &str,
        artist: &str,
        duration_sec: f64,
        is_playing: bool,
        pending_switch: bool,
    ) {
        self.on_playback_state_with_album(
            track_id,
            title,
            artist,
            None,
            duration_sec,
            is_playing,
            pending_switch,
        );
    }

    pub fn on_playback_state_with_album(
        &self,
        track_id: u64,
        title: &str,
        artist: &str,
        album: Option<&str>,
        duration_sec: f64,
        is_playing: bool,
        pending_switch: bool,
    ) {
        let clean_title = clean_metadata_title(title);
        let clean_artist = clean_metadata_artist(artist);
        let clean_album = album.map(clean_metadata_title).filter(|a| !a.is_empty());

        if pending_switch || track_id == 0 || clean_title.is_empty() || clean_artist.is_empty() {
            return;
        }

        let session_key = {
            let guard = self.inner.lock().unwrap();
            if !guard.enabled {
                return;
            }
            match &guard.session {
                Some(s) if !s.session_key.is_empty() => s.session_key.clone(),
                _ => return,
            }
        };

        let mut send_now_playing: Option<(String, String, Option<String>, u64)> = None;
        let mut send_scrobble: Option<PendingScrobbleItem> = None;
        let mut should_flush_queue = false;

        {
            let mut guard = self.inner.lock().unwrap();
            let is_new_track = match &guard.tracker {
                Some(t) => {
                    t.track_id != track_id || t.title != clean_title || t.artist != clean_artist
                }
                None => true,
            };

            if is_new_track {
                let dur = if duration_sec.is_finite() && duration_sec > 0.0 {
                    duration_sec.round() as u64
                } else {
                    0
                };
                guard.tracker = Some(ScrobbleTracker::new(
                    track_id,
                    clean_title.clone(),
                    clean_artist.clone(),
                    clean_album.clone(),
                    dur,
                ));
            }

            if let Some(ref mut tr) = guard.tracker {
                if tr.duration_sec == 0 && duration_sec.is_finite() && duration_sec > 0.0 {
                    tr.duration_sec = duration_sec.round() as u64;
                }
                if tr.album.is_none() && clean_album.is_some() {
                    tr.album = clean_album;
                }

                let now = Instant::now();
                if is_playing {
                    if let Some(prev) = tr.last_tick {
                        let dt = now.duration_since(prev).as_secs_f64();
                        if dt > 0.0 && dt < 5.0 {
                            tr.accumulated_play_sec += dt;
                        }
                    }
                    tr.last_tick = Some(now);

                    let should_refresh_np = tr
                        .last_now_playing_sent
                        .map(|t| t.elapsed() >= Duration::from_secs(25))
                        .unwrap_or(true);

                    if should_refresh_np {
                        tr.last_now_playing_sent = Some(now);
                        send_now_playing = Some((
                            tr.artist.clone(),
                            tr.title.clone(),
                            tr.album.clone(),
                            tr.duration_sec,
                        ));
                    }

                    if !tr.scrobbled && tr.duration_sec >= 30 {
                        let threshold = ((tr.duration_sec as f64) * 0.5).min(240.0);
                        if tr.accumulated_play_sec >= threshold {
                            tr.scrobbled = true;
                            send_scrobble = Some(PendingScrobbleItem {
                                artist: tr.artist.clone(),
                                track: tr.title.clone(),
                                album: tr.album.clone(),
                                duration: tr.duration_sec,
                                timestamp: tr.started_unix,
                            });
                        }
                    }
                } else {
                    tr.last_tick = None;
                }
            }

            if !guard.offline_queue.is_empty()
                && !guard.flushing_queue
                && guard
                    .last_flush_attempt
                    .map(|t| t.elapsed() >= Duration::from_secs(30))
                    .unwrap_or(true)
            {
                should_flush_queue = true;
            }
        }

        if let Some((art, ttl, alb, dur)) = send_now_playing {
            let this = self.clone();
            let sk = session_key.clone();
            thread::spawn(move || {
                this.post_update_now_playing(&sk, &art, &ttl, alb.as_deref(), dur);
            });
        }

        if let Some(item) = send_scrobble {
            let this = self.clone();
            thread::spawn(move || {
                this.post_or_queue_scrobble(&session_key, item);
            });
        } else if should_flush_queue {
            self.spawn_flush_offline_queue();
        }
    }

    fn fetch_auth_token(&self) -> Result<String, String> {
        let creds = resolved_credentials();
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "auth.getToken".to_string());
        params.insert("api_key".to_string(), creds.lastfm_api_key.clone());
        let sig = sign_lastfm_params(&params);

        let url = format!(
            "{}?method=auth.getToken&api_key={}&api_sig={}&format=json",
            LASTFM_API_URL, creds.lastfm_api_key, sig
        );

        let mut last_error = String::new();
        let mut json = serde_json::Value::Null;

        for attempt in 0..2 {
            match self.client.get(&url).send() {
                Ok(resp) => match resp.json() {
                    Ok(parsed) => {
                        json = parsed;
                        last_error.clear();
                        break;
                    }
                    Err(e) => last_error = format!("Geçersiz yanıt: {e}"),
                },
                Err(e) => last_error = format!("Bağlantı hatası: {e}"),
            }

            if attempt == 0 {
                thread::sleep(Duration::from_millis(250));
            }
        }

        if !last_error.is_empty() {
            return Err(last_error);
        }

        if let Some(msg) = json.get("message").and_then(|v| v.as_str()) {
            return Err(format!("Last.fm API Hatası: {msg}"));
        }

        json.get("token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Last.fm token alınamadı.".to_string())
    }

    fn exchange_token_for_session(&self, token: &str) -> Result<LastFmSession, String> {
        let creds = resolved_credentials();
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "auth.getSession".to_string());
        params.insert("api_key".to_string(), creds.lastfm_api_key.clone());
        params.insert("token".to_string(), token.to_string());
        let sig = sign_lastfm_params(&params);

        let url = format!(
            "{}?method=auth.getSession&api_key={}&token={}&api_sig={}&format=json",
            LASTFM_API_URL, creds.lastfm_api_key, token, sig
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .map_err(|e| format!("Bağlantı hatası: {e}"))?;
        let json: serde_json::Value = resp.json().map_err(|e| format!("Geçersiz yanıt: {e}"))?;

        let sess = json
            .get("session")
            .ok_or_else(|| "Henüz onaylanmadı.".to_string())?;
        let username = sess
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let session_key = sess
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        if session_key.is_empty() {
            return Err("Oturum anahtarı boş.".into());
        }

        Ok(LastFmSession {
            username,
            session_key,
            encrypted_session_key: None,
        })
    }

    fn classify_api_response(
        &self,
        resp: Result<reqwest::blocking::Response, reqwest::Error>,
    ) -> Result<bool, ()> {
        let r = match resp {
            Ok(r) => r,
            Err(_) => return Err(()),
        };

        let status_code = r.status().as_u16();
        if status_code >= 500 || status_code == 429 {
            return Err(());
        }

        let Ok(json) = r.json::<serde_json::Value>() else {
            return Err(());
        };

        if let Some(code) = json.get("error").and_then(|v| v.as_u64()) {
            let msg = json
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Bilinmeyen Last.fm hatası");

            match code {
                11 | 16 | 29 => return Err(()),
                9 => {
                    delete_lastfm_session_from_disk();
                    let mut guard = self.inner.lock().unwrap();
                    guard.session = None;
                    guard.tracker = None;
                    guard.status = LastFmAuthStatus::Error(
                        "Last.fm oturumunun süresi dolmuş, lütfen tekrar giriş yap.".into(),
                    );
                    return Ok(false);
                }
                _ => {
                    let mut guard = self.inner.lock().unwrap();
                    guard.status = LastFmAuthStatus::Error(format!("Last.fm ({code}): {msg}"));
                    return Ok(false);
                }
            }
        }

        {
            let mut guard = self.inner.lock().unwrap();
            if matches!(guard.status, LastFmAuthStatus::Error(_))
                && let Some(ref sess) = guard.session
            {
                guard.status = LastFmAuthStatus::LoggedIn {
                    username: sess.username.clone(),
                };
            }
        }

        Ok(true)
    }

    fn post_update_now_playing(
        &self,
        sk: &str,
        artist: &str,
        track: &str,
        album: Option<&str>,
        duration: u64,
    ) {
        let creds = resolved_credentials();
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "track.updateNowPlaying".to_string());
        params.insert("api_key".to_string(), creds.lastfm_api_key.clone());
        params.insert("sk".to_string(), sk.to_string());
        params.insert("artist".to_string(), artist.to_string());
        params.insert("track".to_string(), track.to_string());
        if let Some(alb) = album
            && !alb.is_empty()
        {
            params.insert("album".to_string(), alb.to_string());
        }
        if duration > 0 {
            params.insert("duration".to_string(), duration.to_string());
        }
        let sig = sign_lastfm_params(&params);
        params.insert("api_sig".to_string(), sig);
        params.insert("format".to_string(), "json".to_string());

        let body = encode_form_urlencoded(&params);
        let resp = self
            .client
            .post(LASTFM_API_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send();
        let _ = self.classify_api_response(resp);
    }

    fn post_or_queue_scrobble(&self, sk: &str, item: PendingScrobbleItem) {
        let creds = resolved_credentials();
        let mut params = BTreeMap::new();
        params.insert("method".to_string(), "track.scrobble".to_string());
        params.insert("api_key".to_string(), creds.lastfm_api_key.clone());
        params.insert("sk".to_string(), sk.to_string());
        params.insert("artist".to_string(), item.artist.clone());
        params.insert("track".to_string(), item.track.clone());
        params.insert("timestamp".to_string(), item.timestamp.to_string());
        if let Some(ref alb) = item.album
            && !alb.is_empty()
        {
            params.insert("album".to_string(), alb.clone());
        }
        if item.duration > 0 {
            params.insert("duration".to_string(), item.duration.to_string());
        }
        let sig = sign_lastfm_params(&params);
        params.insert("api_sig".to_string(), sig);
        params.insert("format".to_string(), "json".to_string());

        let body = encode_form_urlencoded(&params);
        let resp = self
            .client
            .post(LASTFM_API_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send();

        match self.classify_api_response(resp) {
            Ok(true) => {
                {
                    let mut guard = self.inner.lock().unwrap();
                    guard.scrobbled_count = guard.scrobbled_count.saturating_add(1);
                    guard.last_scrobbled_track = Some(format!("{} - {}", item.artist, item.track));
                }
                self.spawn_flush_offline_queue();
            }
            Ok(false) => {}
            Err(()) => {
                self.enqueue_offline_scrobble(item);
            }
        }
    }

    fn enqueue_offline_scrobble(&self, item: PendingScrobbleItem) {
        let snapshot = {
            let mut guard = self.inner.lock().unwrap();
            while guard.offline_queue.len() >= SCROBBLE_QUEUE_MAX_ITEMS {
                guard.offline_queue.pop_front();
            }
            guard.offline_queue.push_back(item);
            guard.offline_queue.clone()
        };
        save_scrobble_queue_to_disk(&snapshot);
    }

    fn spawn_flush_offline_queue(&self) {
        let (sk, batch) = {
            let mut guard = self.inner.lock().unwrap();
            if !guard.enabled || guard.flushing_queue || guard.offline_queue.is_empty() {
                return;
            }
            let session_key = match guard.session.as_ref() {
                Some(sess) if !sess.session_key.is_empty() => sess.session_key.clone(),
                _ => return,
            };

            let now_sec = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            guard
                .offline_queue
                .retain(|it| now_sec.saturating_sub(it.timestamp) <= SCROBBLE_MAX_AGE_SEC);

            if guard.offline_queue.is_empty() {
                let empty = guard.offline_queue.clone();
                drop(guard);
                save_scrobble_queue_to_disk(&empty);
                return;
            }

            guard.flushing_queue = true;
            guard.last_flush_attempt = Some(Instant::now());

            let take_count = guard.offline_queue.len().min(SCROBBLE_BATCH_SIZE);
            let batch: Vec<PendingScrobbleItem> = guard
                .offline_queue
                .iter()
                .take(take_count)
                .cloned()
                .collect();
            (session_key, batch)
        };

        let this = self.clone();
        thread::spawn(move || {
            let creds = resolved_credentials();
            let mut params = BTreeMap::new();
            params.insert("method".to_string(), "track.scrobble".to_string());
            params.insert("api_key".to_string(), creds.lastfm_api_key.clone());
            params.insert("sk".to_string(), sk);

            for (idx, item) in batch.iter().enumerate() {
                params.insert(format!("artist[{idx}]"), item.artist.clone());
                params.insert(format!("track[{idx}]"), item.track.clone());
                params.insert(format!("timestamp[{idx}]"), item.timestamp.to_string());
                if let Some(ref alb) = item.album
                    && !alb.is_empty()
                {
                    params.insert(format!("album[{idx}]"), alb.clone());
                }
                if item.duration > 0 {
                    params.insert(format!("duration[{idx}]"), item.duration.to_string());
                }
            }

            let sig = sign_lastfm_params(&params);
            params.insert("api_sig".to_string(), sig);
            params.insert("format".to_string(), "json".to_string());

            let body = encode_form_urlencoded(&params);
            let resp = this
                .client
                .post(LASTFM_API_URL)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body)
                .send();

            let result = this.classify_api_response(resp);
            let (updated_queue, has_more) = {
                let mut guard = this.inner.lock().unwrap();
                guard.flushing_queue = false;
                if matches!(result, Ok(true)) {
                    let sent = batch.len().min(guard.offline_queue.len());
                    for _ in 0..sent {
                        guard.offline_queue.pop_front();
                    }
                    guard.scrobbled_count = guard.scrobbled_count.saturating_add(sent as u64);
                    if let Some(last) = batch.last() {
                        guard.last_scrobbled_track =
                            Some(format!("{} - {}", last.artist, last.track));
                    }
                }
                let q = guard.offline_queue.clone();
                let more = matches!(result, Ok(true)) && !guard.offline_queue.is_empty();
                (q, more)
            };

            if matches!(result, Ok(true)) {
                save_scrobble_queue_to_disk(&updated_queue);
            }
            if has_more {
                this.spawn_flush_offline_queue();
            }
        });
    }
}

fn clean_metadata_title(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    let noisy_tags = [
        "(Explicit)",
        "[Explicit]",
        "(Clean)",
        "[Clean]",
        "(Remastered)",
        "[Remastered]",
        "(Album Version)",
        "[Album Version]",
        "(Radio Edit)",
        "[Radio Edit]",
        "- Explicit",
        "- Clean",
        "- Remastered",
        "- Radio Edit",
    ];
    for tag in noisy_tags {
        if let Some(idx) = s.to_lowercase().find(&tag.to_lowercase()) {
            let candidate = s[..idx].trim();
            if !candidate.is_empty() {
                s = candidate.to_string();
            }
        }
    }
    s.trim_end_matches(['-', ' ']).trim().to_string()
}

fn clean_metadata_artist(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((first, _)) = trimmed.split_once(", ") {
        let primary = first.trim();
        if !primary.is_empty() {
            return primary.to_string();
        }
    }
    trimmed.to_string()
}

fn encode_form_urlencoded(params: &BTreeMap<String, String>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn sign_lastfm_params(params: &BTreeMap<String, String>) -> String {
    let mut raw = String::new();
    for (k, v) in params {
        if k == "format" || k == "callback" {
            continue;
        }
        raw.push_str(k);
        raw.push_str(v);
    }
    raw.push_str(&resolved_credentials().lastfm_shared_secret);
    md5_hex(raw.as_bytes())
}

fn lastfm_session_file_path() -> PathBuf {
    nocturne_data_dir().join("lastfm_session.json")
}

fn lastfm_queue_file_path() -> PathBuf {
    nocturne_data_dir().join("lastfm_scrobble_queue.json")
}

fn load_lastfm_session_from_disk() -> Option<LastFmSession> {
    let data = fs::read_to_string(lastfm_session_file_path()).ok()?;
    let mut parsed: LastFmSession = serde_json::from_str(&data).ok()?;

    #[cfg(windows)]
    if let Some(ref cipher) = parsed.encrypted_session_key
        && let Some(decrypted) = dpapi_unprotect_string(cipher)
        && !decrypted.trim().is_empty()
    {
        parsed.session_key = decrypted;
        return Some(parsed);
    }

    if parsed.session_key.trim().is_empty() {
        return None;
    }

    #[cfg(windows)]
    save_lastfm_session_to_disk(&parsed);

    Some(parsed)
}

fn save_lastfm_session_to_disk(session: &LastFmSession) {
    if session.session_key.trim().is_empty() {
        return;
    }

    #[cfg(windows)]
    let stored = if let Some(cipher) = dpapi_protect_string(&session.session_key) {
        LastFmSession {
            username: session.username.clone(),
            session_key: String::new(),
            encrypted_session_key: Some(cipher),
        }
    } else {
        session.clone()
    };

    #[cfg(not(windows))]
    let stored = session.clone();

    if let Ok(json) = serde_json::to_string_pretty(&stored) {
        atomic_write_json(&lastfm_session_file_path(), &json);
    }
}

fn delete_lastfm_session_from_disk() {
    let _ = fs::remove_file(lastfm_session_file_path());
}

fn load_scrobble_queue_from_disk() -> VecDeque<PendingScrobbleItem> {
    fs::read_to_string(lastfm_queue_file_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<PendingScrobbleItem>>(&raw).ok())
        .map(|mut items| {
            if items.len() > SCROBBLE_QUEUE_MAX_ITEMS {
                let overflow = items.len() - SCROBBLE_QUEUE_MAX_ITEMS;
                items.drain(0..overflow);
            }
            VecDeque::from(items)
        })
        .unwrap_or_default()
}

fn save_scrobble_queue_to_disk(queue: &VecDeque<PendingScrobbleItem>) {
    if queue.is_empty() {
        let _ = fs::remove_file(lastfm_queue_file_path());
        return;
    }
    let vec_ref: Vec<&PendingScrobbleItem> = queue.iter().collect();
    if let Ok(json) = serde_json::to_string_pretty(&vec_ref) {
        atomic_write_json(&lastfm_queue_file_path(), &json);
    }
}

fn open_url_in_default_browser(url: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use widestring::U16CString;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::PCWSTR;

        let Ok(operation) = U16CString::from_str("open") else {
            return false;
        };
        let Ok(target) = U16CString::from_str(url) else {
            return false;
        };

        unsafe {
            let result = ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
            !result.0.is_null()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok()
    }
}

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;

    for chunk in msg.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            let j = i * 4;
            *word = u32::from_le_bytes([chunk[j], chunk[j + 1], chunk[j + 2], chunk[j + 3]]);
        }

        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };
            let temp = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = temp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut out = String::with_capacity(32);
    for val in [a0, b0, c0, d0] {
        for byte in val.to_le_bytes() {
            out.push_str(&format!("{:02x}", byte));
        }
    }
    out
}

// ============================================================================
// GITHUB OTOMATİK GÜNCELLEME DENETLEYİCİSİ
// ============================================================================

const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/egehansimarikk/Nocturne/releases/latest";
const GITHUB_RELEASES_PAGE: &str = "https://github.com/egehansimarikk/Nocturne/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUpdateInfo {
    pub latest_version: String,
    pub download_url: String,
    pub release_url: String,
    pub release_notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckState {
    Idle,
    Checking,
    UpToDate,
    Available(ReleaseUpdateInfo),
    Error(String),
}

struct UpdateCheckerInner {
    state: UpdateCheckState,
}

#[derive(Clone)]
pub struct UpdateChecker {
    inner: Arc<Mutex<UpdateCheckerInner>>,
    client: reqwest::blocking::Client,
}

impl Default for UpdateChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateChecker {
    pub fn new() -> Self {
        let user_agent = format!("Nocturne/{}", env!("CARGO_PKG_VERSION"));
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(10))
            .user_agent(user_agent)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self {
            inner: Arc::new(Mutex::new(UpdateCheckerInner {
                state: UpdateCheckState::Idle,
            })),
            client,
        }
    }

    pub fn current_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn state(&self) -> UpdateCheckState {
        self.inner.lock().unwrap().state.clone()
    }

    pub fn available_update(&self) -> Option<ReleaseUpdateInfo> {
        match &self.inner.lock().unwrap().state {
            UpdateCheckState::Available(info) => Some(info.clone()),
            _ => None,
        }
    }

    pub fn check_in_background(&self, delay: Duration) {
        {
            let mut guard = self.inner.lock().unwrap();
            if matches!(guard.state, UpdateCheckState::Checking) {
                return;
            }
            guard.state = UpdateCheckState::Checking;
        }

        let this = self.clone();
        thread::spawn(move || {
            if !delay.is_zero() {
                thread::sleep(delay);
            }
            let result = this.fetch_latest_release();
            let mut guard = this.inner.lock().unwrap();
            guard.state = result;
        });
    }

    pub fn check_now(&self) {
        self.check_in_background(Duration::ZERO);
    }

    pub fn open_update_download(&self) -> bool {
        let target_url = match &self.inner.lock().unwrap().state {
            UpdateCheckState::Available(info) => info.download_url.clone(),
            _ => GITHUB_RELEASES_PAGE.to_string(),
        };
        open_url_in_default_browser(&target_url)
    }

    fn fetch_latest_release(&self) -> UpdateCheckState {
        let resp = match self
            .client
            .get(GITHUB_LATEST_RELEASE_API)
            .header("Accept", "application/vnd.github+json")
            .send()
        {
            Ok(r) => r,
            Err(e) => return UpdateCheckState::Error(format!("Bağlantı hatası: {e}")),
        };

        let status = resp.status().as_u16();
        if status == 404 {
            return UpdateCheckState::UpToDate;
        }
        if status >= 400 {
            return UpdateCheckState::Error(format!("GitHub API yanıt kodu: {status}"));
        }

        let json: serde_json::Value = match resp.json() {
            Ok(v) => v,
            Err(e) => return UpdateCheckState::Error(format!("Geçersiz JSON yanıtı: {e}")),
        };

        let tag_name = json
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim();
        if tag_name.is_empty() {
            return UpdateCheckState::UpToDate;
        }

        let current_ver = env!("CARGO_PKG_VERSION");
        if !is_version_newer(tag_name, current_ver) {
            return UpdateCheckState::UpToDate;
        }

        let release_url = json
            .get("html_url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(GITHUB_RELEASES_PAGE)
            .to_string();

        let release_notes = json
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();

        let download_url =
            select_windows_installer_asset(&json).unwrap_or_else(|| release_url.clone());

        UpdateCheckState::Available(ReleaseUpdateInfo {
            latest_version: tag_name.trim_start_matches(['v', 'V']).to_string(),
            download_url,
            release_url,
            release_notes,
        })
    }
}

fn select_windows_installer_asset(json: &serde_json::Value) -> Option<String> {
    let assets = json.get("assets")?.as_array()?;
    let mut fallback_exe: Option<String> = None;

    for asset in assets {
        let name = asset
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let url = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if url.is_empty() || !name.ends_with(".exe") {
            continue;
        }

        if name.contains("setup") || name.contains("installer") {
            return Some(url.to_string());
        }
        if fallback_exe.is_none() {
            fallback_exe = Some(url.to_string());
        }
    }

    fallback_exe
}

fn parse_semver_tuple(raw: &str) -> (u64, u64, u64) {
    let cleaned = raw
        .trim()
        .trim_start_matches(['v', 'V'])
        .split(['-', '+'])
        .next()
        .unwrap_or_default();

    let mut parts = cleaned.split('.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

fn is_version_newer(candidate: &str, current: &str) -> bool {
    parse_semver_tuple(candidate) > parse_semver_tuple(current)
}
