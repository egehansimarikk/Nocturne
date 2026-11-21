#![windows_subsystem = "windows"]

mod audio_engine;
mod i18n;
mod icons;
mod integrations;
mod settings_modal;
mod theme;
mod webview_bridge;

use audio_engine::{AudioDiagnostics, AudioEngine, AudioOutputDevice, OutputShareMode};
use gpui::{
    div, img, prelude::*, px, rgb, rgba, size, App, Bounds, Context, ElementId, FocusHandle,
    Focusable, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, NavigationDirection, ParentElement, Render, Rgba, SharedString, Styled,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use i18n::{text, LanguageSetting};
use icons::{lucide_icon, NocturneAssets};
use integrations::{DiscordRpcManager, LastFmManager, UpdateChecker};
use serde::{Deserialize, Serialize};
use settings_modal::SettingsTab;
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use theme::{ThemeId, ThemePalette};
use webview_bridge::{AudioQuality, DeezerEngine, EngineEvent, PlaybackState, UserPlaylist};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    fn next(self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }

    fn icon_name(self) -> &'static str {
        match self {
            RepeatMode::One => "repeat-one",
            RepeatMode::Off | RepeatMode::All => "repeat",
        }
    }

    fn label(self, language: LanguageSetting) -> String {
        let value = match self {
            RepeatMode::Off => "Kapalı",
            RepeatMode::All => "Liste",
            RepeatMode::One => "Tek",
        };
        format!("{}: {}", text(language, "Tekrar"), text(language, value))
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppConfig {
    volume: f32,
    exclusive_mode: bool,
    #[serde(default)]
    selected_device_id: Option<String>,
    #[serde(default)]
    theme: ThemeId,
    eq_enabled: bool,
    eq_preset_name: String,
    eq_bands: [f32; 10],
    #[serde(default)]
    eq_preamp_db: f32,
    #[serde(default)]
    language: LanguageSetting,
    #[serde(default)]
    audio_quality: AudioQuality,
    #[serde(default)]
    normalize_volume: bool,
    #[serde(default = "default_true")]
    discord_rpc_enabled: bool,
    #[serde(default = "default_true")]
    lastfm_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            exclusive_mode: false,
            selected_device_id: None,
            theme: ThemeId::Obsidian,
            eq_enabled: true,
            eq_preset_name: "Harman Over-Ear".to_string(),
            eq_bands: [3.2, 2.5, 0.8, -0.5, 0.0, 0.5, 1.5, 2.2, 1.8, 1.0],
            eq_preamp_db: 0.0,
            language: LanguageSetting::System,
            audio_quality: AudioQuality::HiFi,
            normalize_volume: false,
            discord_rpc_enabled: true,
            lastfm_enabled: true,
        }
    }
}

impl AppConfig {
    fn path() -> PathBuf {
        data_dir().join("config.json")
    }

    fn load() -> Self {
        let path = Self::path();
        if let Ok(raw) = fs::read_to_string(path)
            && let Ok(cfg) = serde_json::from_str::<AppConfig>(&raw)
        {
            return cfg;
        }
        Self::default()
    }

    fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let target = Self::path();
            let tmp = target.with_extension("json.tmp");
            if fs::write(&tmp, &json).is_ok() {
                if fs::rename(&tmp, &target).is_err() {
                    let _ = fs::write(&target, &json);
                    let _ = fs::remove_file(&tmp);
                }
            } else {
                let _ = fs::write(&target, &json);
            }
        }
    }
}

const EQ_PREAMP_MIN: f32 = -12.0;
const EQ_PREAMP_MAX: f32 = 6.0;
const EQ_PREAMP_STEP: f32 = 0.5;
const SEARCH_HISTORY_LIMIT: usize = 8;
const NAV_HISTORY_LIMIT: usize = 30;
const PLAYLIST_VERIFY_TIMEOUT: Duration = Duration::from_secs(16);
const VOLUME_SLIDER_WIDTH: f32 = 92.0;

fn data_dir() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Nocturne");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn clamp_preamp_db(value: f32) -> f32 {
    let stepped = (value / EQ_PREAMP_STEP).round() * EQ_PREAMP_STEP;
    stepped.clamp(EQ_PREAMP_MIN, EQ_PREAMP_MAX)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqPreset {
    name: String,
    #[serde(default)]
    bands: [f32; 10],
    #[serde(default)]
    preamp_db: f32,
}

fn presets_path() -> PathBuf {
    data_dir().join("presets.json")
}

fn load_eq_presets() -> Vec<EqPreset> {
    fs::read_to_string(presets_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<EqPreset>>(&raw).ok())
        .map(|presets| {
            presets
                .into_iter()
                .map(|mut preset| {
                    preset.name = preset.name.trim().to_string();
                    preset.bands = preset.bands.map(|gain| gain.clamp(-12.0, 12.0));
                    preset.preamp_db = clamp_preamp_db(preset.preamp_db);
                    preset
                })
                .filter(|preset| !preset.name.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn save_eq_presets(presets: &[EqPreset]) {
    if let Ok(json) = serde_json::to_string_pretty(presets) {
        let _ = fs::write(presets_path(), json);
    }
}

const ARTWORK_CACHE_LIMIT: usize = 320;
const ARTWORK_CACHE_MAX_AGE_DAYS: u64 = 30;

fn cleanup_artwork_cache() {
    let cache_dir = data_dir().join("ArtworkCache");
    let Ok(entries) = fs::read_dir(&cache_dir) else {
        return;
    };

    let mut files: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();

    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    let max_age = Duration::from_secs(ARTWORK_CACHE_MAX_AGE_DAYS * 24 * 60 * 60);
    let now = SystemTime::now();

    for (index, (modified, path)) in files.into_iter().enumerate() {
        let stale = now.duration_since(modified).unwrap_or_default() > max_age;
        if index >= ARTWORK_CACHE_LIMIT || stale {
            let _ = fs::remove_file(path);
        }
    }
}

struct ThumbLoader {
    queue: Arc<(Mutex<VecDeque<String>>, Condvar)>,
}

impl ThumbLoader {
    fn new(tx: Sender<BgTaskResult>) -> Self {
        let queue = Arc::new((Mutex::new(VecDeque::<String>::new()), Condvar::new()));
        let cache_dir = data_dir().join("ArtworkCache");
        let _ = fs::create_dir_all(&cache_dir);

        for _ in 0..4 {
            let q = Arc::clone(&queue);
            let tx_worker = tx.clone();
            let dir = cache_dir.clone();
            thread::spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .unwrap_or_else(|_| reqwest::blocking::Client::new());

                loop {
                    let url = {
                        let (lock, cvar) = &*q;
                        let mut guard = match lock.lock() {
                            Ok(g) => g,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        while guard.is_empty() {
                            guard = match cvar.wait(guard) {
                                Ok(g) => g,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                        }
                        guard.pop_front()
                    };

                    let Some(url) = url else { continue };
                    let target_path = Self::cache_path_for(&dir, &url);
                    if target_path.exists() {
                        let _ = tx_worker.send(BgTaskResult::ThumbCached(url, target_path));
                        continue;
                    }

                    if let Ok(resp) = client.get(&url).send()
                        && resp.status().is_success()
                        && let Ok(bytes) = resp.bytes()
                        && fs::write(&target_path, &bytes).is_ok()
                    {
                        let _ = tx_worker.send(BgTaskResult::ThumbCached(url, target_path));
                    }
                }
            });
        }

        Self { queue }
    }

    fn cache_path_for(cache_dir: &Path, url: &str) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        cache_dir.join(format!("art_{:x}.jpg", hasher.finish()))
    }

    fn enqueue(&self, url: String) {
        let (lock, cvar) = &*self.queue;
        let mut guard = match lock.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push_back(url);
        cvar.notify_one();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BrowseRoute {
    Search { query: String },
    Category { query: String, label: String },
    LovedTracks,
    Playlist { id: u64, title: String },
    Artist { id: u64, name: String },
    Album { id: u64, title: String },
}

#[derive(Debug, Clone, Copy)]
struct NavItemDef {
    id: &'static str,
    label: &'static str,
    query: &'static str,
    cat_title: &'static str,
}

static NAV_PHONK: NavItemDef = NavItemDef {
    id: "nav-phonk",
    label: "Drift Phonk",
    query: "Drift Phonk",
    cat_title: "Keşfet • Drift Phonk",
};

static NAV_CAUCASIAN: NavItemDef = NavItemDef {
    id: "nav-caucasian",
    label: "Caucasian & Russian",
    query: "Babek Mamedrzaev",
    cat_title: "Keşfet • Caucasian & Russian",
};

static NAV_DARKPOP: NavItemDef = NavItemDef {
    id: "nav-darkpop",
    label: "Dark Pop & Trap",
    query: "Dark Pop",
    cat_title: "Keşfet • Dark Pop & Trap",
};

static NAV_FLAC_TEST: NavItemDef = NavItemDef {
    id: "nav-flac-test",
    label: "Hi-Fi / Audiophile Test",
    query: "Daft Punk Random Access Memories",
    cat_title: "Hi-Fi / Audiophile Test",
};

static NAV_TR_RAP: NavItemDef = NavItemDef {
    id: "nav-tr-rap",
    label: "Türkçe Hip-Hop",
    query: "Türkçe Rap",
    cat_title: "Keşfet • Türkçe Hip-Hop",
};

fn search_history_path() -> PathBuf {
    data_dir().join("search_history.json")
}

fn load_search_history() -> Vec<String> {
    fs::read_to_string(search_history_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .map(|mut items| {
            items.retain(|item| !item.trim().is_empty());
            items.truncate(SEARCH_HISTORY_LIMIT);
            items
        })
        .unwrap_or_default()
}

fn save_search_history(history: &[String]) {
    if let Ok(json) = serde_json::to_string_pretty(history) {
        let _ = fs::write(search_history_path(), json);
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerSearchResponse {
    #[serde(default)]
    data: Vec<DeezerTrackItem>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeezerAlbumDetailResponse {
    #[serde(default)]
    title: String,
    #[serde(default)]
    cover_medium: String,
    #[serde(default)]
    tracks: Option<DeezerSearchResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeezerTrackItem {
    pub id: u64,
    pub title: String,
    pub duration: u64,
    pub artist: DeezerArtistInfo,
    #[serde(default)]
    pub album: DeezerAlbumInfo,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeezerArtistInfo {
    #[serde(default)]
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeezerAlbumInfo {
    #[serde(default)]
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cover_medium: String,
}

pub enum BgTaskResult {
    SearchResults(Vec<DeezerTrackItem>),
    SearchPage(Vec<DeezerTrackItem>, Option<String>),
    LovedTrackIds(Vec<u64>),
    ArtworkCached(String, PathBuf, Option<u32>),
    ThumbCached(String, PathBuf),
    AudioDevices(Vec<AudioOutputDevice>),
}

pub struct NocturneApp {
    pub focus_handle: FocusHandle,
    pub engine: DeezerEngine,
    pub audio: AudioEngine,
    pub discord_rpc: DiscordRpcManager,
    pub lastfm: LastFmManager,
    pub updater: UpdateChecker,
    pub state: PlaybackState,
    pub bg_tx: Sender<BgTaskResult>,
    pub bg_rx: Receiver<BgTaskResult>,

    // Tema & Dinamik Kapak Rengi
    pub theme_id: ThemeId,
    pub cover_ambient_rgb: Option<u32>,

    // Arama & Katalog & Çalma Listeleri
    pub search_query: String,
    pub search_focused: bool,
    pub is_searching: bool,
    pub search_history: Vec<String>,
    pub next_page_url: Option<String>,
    pub loading_more: bool,
    pub tracks: Vec<DeezerTrackItem>,
    pub user_playlists: Vec<UserPlaylist>,
    pub deezer_user_id: Option<u64>,
    pub selected_playlist_id: Option<u64>,
    pub playlist_picker_track: Option<u64>,
    pub new_playlist_open: bool,
    pub new_playlist_input: String,
    pub new_playlist_error: Option<String>,
    pub new_playlist_track: Option<u64>,
    pub pending_playlist_title: Option<String>,
    pub pending_playlist_since: Option<Instant>,
    pub delete_confirm_playlist: Option<(u64, Instant)>,
    pub action_notice: Option<(String, Instant)>,
    pub liked_track_ids: HashSet<u64>,
    pub loading_loved_tracks: bool,
    pub active_category: SharedString,

    // Görünüm Geçmişi (Geri / İleri Navigasyon)
    nav_history: Vec<BrowseRoute>,
    nav_cursor: usize,

    // Satır Kapak Önbelleği
    thumb_loader: ThumbLoader,
    pub thumb_cache: HashMap<String, PathBuf>,
    thumb_requested: HashSet<String>,

    // Akıllı Sıra (Queue) & Otomatik Geçiş
    pub queue: Vec<DeezerTrackItem>,
    pub queue_index: Option<usize>,
    pub target_track_id: u64,
    pub shuffle_enabled: bool,
    pub repeat_mode: RepeatMode,
    pub queue_drawer_open: bool,
    pub last_track_advance: Instant,
    pub last_seek_at: Instant,
    pub startup_refresh_at: Option<Instant>,
    pub startup_refresh_stage: u8,
    pub pending_eq_reapply_until: Option<Instant>,

    // Görsel & Ses Durumu
    pub cached_artwork_url: String,
    pub cached_artwork_path: Option<PathBuf>,
    pub volume: f32,
    pub muted: bool,
    pub volume_before_mute: f32,
    pub volume_dragging: bool,

    // Tek Birleşik Ayarlar Penceresi
    pub settings_modal_open: bool,
    pub settings_tab: SettingsTab,
    pub language: LanguageSetting,
    pub output_devices: Vec<AudioOutputDevice>,
    pub selected_device_id: Option<String>,
    pub share_mode: OutputShareMode,
    pub audio_diag: AudioDiagnostics,
    pub audio_quality: AudioQuality,
    pub normalize_volume: bool,
    pub eq_enabled: bool,
    pub eq_preset_name: SharedString,
    pub eq_bands: [f32; 10],
    pub eq_preamp_db: f32,
    pub eq_presets: Vec<EqPreset>,
    pub eq_preset_input: String,
    pub preset_name_focused: bool,
    pub login_window_open: bool,
}

impl NocturneApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let cfg = AppConfig::load();
        let initial_mode = OutputShareMode::Shared;
        let init_preamp = clamp_preamp_db(cfg.eq_preamp_db);
        let init_vol = cfg.volume.clamp(0.0, 1.0);

        // Başlangıç EQ kazançları, ses seviyesi ve akış kalitesi WebView2 doğar doğmaz enjeksiyon betiğine gömülür.
        let engine = DeezerEngine::spawn_with_eq(
            cfg.eq_enabled,
            cfg.eq_bands,
            init_preamp,
            initial_mode == OutputShareMode::Shared,
            init_vol,
            cfg.audio_quality,
            cfg.normalize_volume,
        );
        let audio = AudioEngine::start(
            initial_mode,
            cfg.eq_enabled,
            cfg.eq_bands,
            init_preamp,
            cfg.selected_device_id.clone(),
        );
        let discord_rpc = DiscordRpcManager::new(cfg.discord_rpc_enabled);
        let lastfm = LastFmManager::new();
        lastfm.set_enabled(cfg.lastfm_enabled);
        let updater = UpdateChecker::new();
        updater.check_in_background(Duration::from_secs(3));

        let (bg_tx, bg_rx) = mpsc::channel();
        let thumb_loader = ThumbLoader::new(bg_tx.clone());

        let initial_category = format!("{} • Drift Phonk", text(cfg.language, "Keşfet"));
        let initial_route = BrowseRoute::Category {
            query: "Phonk".to_string(),
            label: initial_category.clone(),
        };

        Self::spawn_search("Phonk", bg_tx.clone());
        Self::spawn_device_scan(bg_tx.clone());
        thread::spawn(cleanup_artwork_cache);

        let init_bands = cfg.eq_bands;
        let init_eq_enabled = cfg.eq_enabled;

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;

                let ok = this.update(cx, |app, cx| {
                    app.engine.pump();

                    while let Ok(evt) = app.engine.rx.try_recv() {
                        match evt {
                            EngineEvent::State(st) => {
                                let matches_target = if app.target_track_id == 0 {
                                    st.is_playing && !st.title.is_empty()
                                } else {
                                    st.track_id == app.target_track_id
                                        || (st.track_id == 0
                                            && app.last_track_advance.elapsed()
                                                > Duration::from_secs(3))
                                };

                                if !st.pending_switch && matches_target {
                                    let track_changed = st.track_id > 0
                                        && st.track_id != app.state.track_id;
                                    let started_playing =
                                        st.is_playing && !app.state.is_playing;
                                    if track_changed {
                                        app.cached_artwork_url.clear();
                                        app.cached_artwork_path = None;
                                        app.cover_ambient_rgb = None;
                                    }

                                    if st.track_id > 0 {
                                        app.state.track_id = st.track_id;
                                    }
                                    if !st.title.is_empty() {
                                        app.state.title = st.title;
                                    }
                                    if !st.artist.is_empty() {
                                        app.state.artist = st.artist;
                                    }

                                    if !st.artwork.is_empty()
                                        && st.artwork != app.cached_artwork_url
                                    {
                                        app.cached_artwork_url = st.artwork.clone();
                                        app.cached_artwork_path = None;
                                        app.cover_ambient_rgb = None;
                                        Self::spawn_artwork_download(
                                            st.artwork.clone(),
                                            app.bg_tx.clone(),
                                        );
                                    }

                                    if st.duration > 0.0 {
                                        app.state.duration = app
                                            .queue
                                            .iter()
                                            .find(|track| track.id == st.track_id)
                                            .map(|track| track.duration as f64)
                                            .filter(|duration| *duration > 0.0)
                                            .unwrap_or(st.duration);
                                    }

                                    let seek_settled =
                                        app.last_seek_at.elapsed() > Duration::from_millis(650);

                                    if seek_settled {
                                        app.state.current_time = st.current_time;
                                    }

                                    if track_changed || started_playing {
                                        app.sync_eq_live();
                                        app.engine
                                            .set_volume(if app.muted { 0.0 } else { app.volume });
                                        app.engine.send_command("eq_now");
                                        app.audio.notify_playback_started();
                                        app.pending_eq_reapply_until =
                                            Some(Instant::now() + Duration::from_secs(4));
                                    }
                                    app.state.is_playing = st.is_playing;

                                    // Discord Rich Presence & Last.fm Scrobbling senkronizasyonu
                                    let active_artwork = if !st.artwork.is_empty() {
                                        st.artwork.as_str()
                                    } else {
                                        app.cached_artwork_url.as_str()
                                    };
                                    app.discord_rpc.on_playback_state(
                                        app.state.track_id,
                                        &app.state.title,
                                        &app.state.artist,
                                        active_artwork,
                                        app.state.current_time,
                                        app.state.duration,
                                        app.state.is_playing,
                                        st.pending_switch,
                                    );
                                    app.lastfm.on_playback_state(
                                        app.state.track_id,
                                        &app.state.title,
                                        &app.state.artist,
                                        app.state.duration,
                                        app.state.is_playing,
                                        st.pending_switch,
                                    );

                                    let near_end = seek_settled
                                        && app.state.duration > 3.0
                                        && st.current_time >= app.state.duration - 0.45;
                                    let ended_stopped = seek_settled
                                        && app.state.duration > 3.0
                                        && !st.is_playing
                                        && st.current_time >= app.state.duration - 1.2;

                                    if (near_end || ended_stopped)
                                        && app.last_track_advance.elapsed() > Duration::from_secs(2)
                                    {
                                        app.handle_track_ended();
                                    }
                                }
                                app.state.volume = st.volume;
                            }
                            EngineEvent::EqApplied(applied) => {
                                app.audio.set_web_eq_applied(applied);
                                if applied {
                                    app.pending_eq_reapply_until = None;
                                }
                            }
                            EngineEvent::QualityApplied(_) => {}
                            EngineEvent::Spectrum(_) => {}
                            EngineEvent::TrackEnded => {
                                if app.last_track_advance.elapsed() > Duration::from_secs(2) {
                                    app.handle_track_ended();
                                }
                            }
                            EngineEvent::Playlists(lists) => {
                                if !lists.is_empty() {
                                    app.user_playlists = lists;
                                }
                                app.verify_pending_playlist();
                            }
                            EngineEvent::UserId(uid) => {
                                if uid > 0 && app.deezer_user_id != Some(uid) {
                                    app.deezer_user_id = Some(uid);
                                    Self::spawn_loved_track_ids(uid, app.bg_tx.clone());
                                }
                            }
                            EngineEvent::ActionResult {
                                success,
                                message,
                                kind,
                                track_id,
                                playlist_id,
                            } => {
                                if success && kind == "favorite" && track_id > 0 {
                                    app.liked_track_ids.insert(track_id);
                                } else if success && kind == "unfavorite" && track_id > 0 {
                                    app.liked_track_ids.remove(&track_id);
                                    if app
                                        .active_category
                                        .as_ref()
                                        .starts_with(&text(app.language, "Beğenilen Şarkılar"))
                                    {
                                        app.tracks.retain(|t| t.id != track_id);
                                    }
                                } else if success
                                    && kind == "playlist_track_removed"
                                    && track_id > 0
                                {
                                    if app.selected_playlist_id == Some(playlist_id) {
                                        app.tracks.retain(|t| t.id != track_id);
                                    }
                                    if let Some(pl) = app
                                        .user_playlists
                                        .iter_mut()
                                        .find(|pl| pl.id == playlist_id)
                                    {
                                        pl.nb_tracks = pl.nb_tracks.saturating_sub(1);
                                    }
                                }
                                if kind == "playlist_created" {
                                    app.new_playlist_open = false;
                                    app.new_playlist_input.clear();
                                    app.new_playlist_error = None;
                                    app.new_playlist_track = None;
                                    app.pending_playlist_title = None;
                                    app.pending_playlist_since = None;
                                } else if kind == "playlist_create_error" {
                                    app.pending_playlist_since.get_or_insert_with(Instant::now);
                                    app.new_playlist_error = Some(text(app.language, &message));
                                } else if kind == "playlist_deleted" {
                                    app.delete_confirm_playlist = None;
                                    if success {
                                        app.user_playlists.retain(|pl| pl.id != track_id);
                                        if app.selected_playlist_id == Some(track_id) {
                                            app.selected_playlist_id = None;
                                            app.loading_loved_tracks = false;
                                            app.is_searching = false;
                                            app.tracks.clear();
                                            app.active_category =
                                                text(app.language, "Çalma Listeleri").into();
                                        }
                                    }
                                }
                                if kind != "playlist_create_error" {
                                    let shown_message = if success {
                                        match kind.as_str() {
                                            "favorite" => {
                                                text(app.language, "Şarkı beğenilenlere eklendi.")
                                            }
                                            "unfavorite" => text(
                                                app.language,
                                                "Şarkı beğenilenlerden çıkarıldı.",
                                            ),
                                            "playlist_created" => {
                                                text(app.language, "Çalma listesi oluşturuldu.")
                                            }
                                            "playlist_deleted" => {
                                                text(app.language, "Çalma listesi silindi.")
                                            }
                                            "playlist_track_removed" => text(
                                                app.language,
                                                "Şarkı çalma listesinden çıkarıldı.",
                                            ),
                                            _ => text(
                                                app.language,
                                                "Şarkı çalma listesine eklendi.",
                                            ),
                                        }
                                    } else {
                                        let localized_err = text(app.language, &message);
                                        format!("{} {localized_err}", text(app.language, "Hata:"))
                                    };
                                    app.action_notice = Some((shown_message, Instant::now()));
                                }
                                if success {
                                    app.engine.refresh_playlists();
                                }
                            }
                            EngineEvent::Ready => {
                                app.engine.refresh_playlists();
                                app.sync_eq_live();
                                app.engine.set_volume(if app.muted { 0.0 } else { app.volume });
                                app.engine
                                    .set_audio_quality(app.audio_quality, app.normalize_volume);
                                app.startup_refresh_stage = 0;
                                app.startup_refresh_at =
                                    Some(Instant::now() + Duration::from_millis(250));
                            }
                        }
                    }

                    while let Ok(res) = app.bg_rx.try_recv() {
                        match res {
                            BgTaskResult::SearchResults(items) => {
                                if app.loading_loved_tracks {
                                    app.liked_track_ids
                                        .extend(items.iter().map(|track| track.id));
                                    app.loading_loved_tracks = false;
                                }
                                app.queue_thumbnails_for(&items);
                                app.tracks = items;
                                app.is_searching = false;
                                app.next_page_url = None;
                                app.loading_more = false;
                            }
                            BgTaskResult::SearchPage(items, next) => {
                                app.queue_thumbnails_for(&items);
                                if app.loading_more {
                                    Self::push_unique_tracks(&mut app.tracks, items);
                                } else {
                                    app.tracks.clear();
                                    Self::push_unique_tracks(&mut app.tracks, items);
                                    app.is_searching = false;
                                }
                                app.loading_more = false;
                                app.next_page_url = next;
                            }
                            BgTaskResult::LovedTrackIds(ids) => {
                                app.liked_track_ids.extend(ids);
                            }
                            BgTaskResult::ArtworkCached(url, path, ambient) => {
                                app.thumb_cache.insert(url.clone(), path.clone());
                                if url == app.cached_artwork_url {
                                    app.cached_artwork_path = Some(path);
                                    app.cover_ambient_rgb = ambient;
                                }
                            }
                            BgTaskResult::ThumbCached(url, path) => {
                                app.thumb_cache.insert(url, path);
                            }
                            BgTaskResult::AudioDevices(devs) => {
                                app.output_devices = devs;
                            }
                        }
                    }

                    if app
                        .action_notice
                        .as_ref()
                        .is_some_and(|(_, created)| created.elapsed() > Duration::from_secs(4))
                    {
                        app.action_notice = None;
                    }

                    app.check_pending_playlist_timeout();

                    if app
                        .delete_confirm_playlist
                        .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(4))
                    {
                        app.delete_confirm_playlist = None;
                    }

                    // Açılışta oturum ve çalma listelerini gecikmesiz yokla
                    if app
                        .startup_refresh_at
                        .is_some_and(|at| Instant::now() >= at)
                    {
                        let session_ready = app.deezer_user_id.is_some();
                        let playlists_ready = !app.user_playlists.is_empty();
                        let attempts_exhausted = app.startup_refresh_stage >= 16;

                        app.engine.refresh_playlists();

                        if (session_ready && playlists_ready) || attempts_exhausted {
                            app.startup_refresh_at = None;
                        } else {
                            app.startup_refresh_stage =
                                app.startup_refresh_stage.saturating_add(1);
                            let delay_ms = if session_ready {
                                2500
                            } else if app.startup_refresh_stage <= 8 {
                                400
                            } else {
                                1200
                            };
                            app.startup_refresh_at =
                                Some(Instant::now() + Duration::from_millis(delay_ms));
                        }
                    }

                    // İlk parçada WebView2 ses elementi oluşana kadar EQ'yu canlı tut
                    if let Some(until) = app.pending_eq_reapply_until {
                        if Instant::now() <= until {
                            if app.state.is_playing {
                                app.sync_eq_live();
                                app.engine.send_command("eq_now");
                            }
                        } else {
                            app.pending_eq_reapply_until = None;
                        }
                    }

                    let audio_diag = app.audio.get_diagnostics();
                    if audio_diag.active_mode != app.audio_diag.active_mode {
                        app.sync_eq_live();
                    }
                    app.audio_diag = audio_diag;
                    app.share_mode = app.audio_diag.active_mode;

                    cx.notify();
                });

                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            engine,
            audio,
            discord_rpc,
            lastfm,
            updater,
            state: PlaybackState::default(),
            bg_tx,
            bg_rx,
            theme_id: cfg.theme,
            cover_ambient_rgb: None,
            search_query: String::new(),
            search_focused: true,
            is_searching: true,
            search_history: load_search_history(),
            next_page_url: None,
            loading_more: false,
            tracks: Vec::new(),
            user_playlists: Vec::new(),
            deezer_user_id: None,
            selected_playlist_id: None,
            playlist_picker_track: None,
            new_playlist_open: false,
            new_playlist_input: String::new(),
            new_playlist_error: None,
            new_playlist_track: None,
            pending_playlist_title: None,
            pending_playlist_since: None,
            delete_confirm_playlist: None,
            action_notice: None,
            liked_track_ids: HashSet::new(),
            loading_loved_tracks: false,
            active_category: initial_category.into(),
            nav_history: vec![initial_route],
            nav_cursor: 0,
            thumb_loader,
            thumb_cache: HashMap::new(),
            thumb_requested: HashSet::new(),
            queue: Vec::new(),
            queue_index: None,
            target_track_id: 0,
            shuffle_enabled: false,
            repeat_mode: RepeatMode::All,
            queue_drawer_open: false,
            last_track_advance: Instant::now() - Duration::from_secs(10),
            last_seek_at: Instant::now() - Duration::from_secs(10),
            startup_refresh_at: Some(Instant::now() + Duration::from_millis(350)),
            startup_refresh_stage: 0,
            pending_eq_reapply_until: None,
            cached_artwork_url: String::new(),
            cached_artwork_path: None,
            volume: init_vol,
            muted: false,
            volume_before_mute: init_vol,
            volume_dragging: false,
            settings_modal_open: false,
            settings_tab: SettingsTab::Appearance,
            language: cfg.language,
            output_devices: Vec::new(),
            selected_device_id: cfg.selected_device_id,
            share_mode: initial_mode,
            audio_diag: AudioDiagnostics::default(),
            audio_quality: cfg.audio_quality,
            normalize_volume: cfg.normalize_volume,
            eq_enabled: init_eq_enabled,
            eq_preset_name: cfg.eq_preset_name.into(),
            eq_bands: init_bands,
            eq_preamp_db: init_preamp,
            eq_presets: load_eq_presets(),
            eq_preset_input: String::new(),
            preset_name_focused: false,
            login_window_open: false,
        }
    }

    pub fn spawn_device_scan(tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let devs = AudioEngine::enumerate_output_devices();
            let _ = tx.send(BgTaskResult::AudioDevices(devs));
        });
    }

    fn queue_thumbnails_for(&mut self, items: &[DeezerTrackItem]) {
        for track in items {
            let url = track.album.cover_medium.trim();
            if url.is_empty()
                || self.thumb_cache.contains_key(url)
                || !self.thumb_requested.insert(url.to_string())
            {
                continue;
            }
            self.thumb_loader.enqueue(url.to_string());
        }
    }

    fn can_navigate_back(&self) -> bool {
        self.nav_cursor > 0
    }

    fn can_navigate_forward(&self) -> bool {
        self.nav_cursor + 1 < self.nav_history.len()
    }

    fn push_route(&mut self, route: BrowseRoute) {
        if self.nav_history.get(self.nav_cursor) == Some(&route) {
            return;
        }
        self.nav_history.truncate(self.nav_cursor + 1);
        self.nav_history.push(route);
        if self.nav_history.len() > NAV_HISTORY_LIMIT {
            let overflow = self.nav_history.len() - NAV_HISTORY_LIMIT;
            self.nav_history.drain(0..overflow);
        }
        self.nav_cursor = self.nav_history.len().saturating_sub(1);
    }

    fn navigate_back(&mut self) {
        if !self.can_navigate_back() {
            return;
        }
        self.nav_cursor -= 1;
        if let Some(route) = self.nav_history.get(self.nav_cursor).cloned() {
            self.apply_route(route, false);
        }
    }

    fn navigate_forward(&mut self) {
        if !self.can_navigate_forward() {
            return;
        }
        self.nav_cursor += 1;
        if let Some(route) = self.nav_history.get(self.nav_cursor).cloned() {
            self.apply_route(route, false);
        }
    }

    fn apply_route(&mut self, route: BrowseRoute, record: bool) {
        self.search_focused = false;
        self.loading_more = false;
        self.next_page_url = None;

        if record {
            self.push_route(route.clone());
        }

        match route {
            BrowseRoute::Search { query } => {
                self.search_query = query.clone();
                self.loading_loved_tracks = false;
                self.selected_playlist_id = None;
                self.active_category =
                    format!("{} {}", text(self.language, "Arama:"), query).into();
                self.is_searching = true;
                Self::spawn_search(&query, self.bg_tx.clone());
            }
            BrowseRoute::Category { query, label } => {
                self.loading_loved_tracks = false;
                self.selected_playlist_id = None;
                self.active_category = label.into();
                self.is_searching = true;
                Self::spawn_search(&query, self.bg_tx.clone());
            }
            BrowseRoute::LovedTracks => {
                self.loading_loved_tracks = true;
                self.selected_playlist_id = None;
                self.active_category = text(self.language, "Beğenilen Şarkılar").into();
                if let Some(uid) = self.deezer_user_id {
                    self.is_searching = true;
                    Self::spawn_loved_tracks(uid, self.bg_tx.clone());
                } else {
                    self.is_searching = false;
                    self.engine.refresh_playlists();
                    self.active_category = format!(
                        "{} ({} Deezer)",
                        text(self.language, "Beğenilen Şarkılar"),
                        text(self.language, "Deezer Oturumu")
                    )
                    .into();
                }
            }
            BrowseRoute::Playlist { id, title } => {
                self.loading_loved_tracks = false;
                self.selected_playlist_id = Some(id);
                self.active_category =
                    format!("{} • {}", text(self.language, "Çalma Listesi"), title).into();
                self.is_searching = true;
                Self::spawn_playlist_tracks(id, self.bg_tx.clone());
            }
            BrowseRoute::Artist { id, name } => {
                self.selected_playlist_id = None;
                self.loading_loved_tracks = false;
                self.active_category =
                    format!("{} • {}", text(self.language, "Sanatçı"), name).into();
                self.is_searching = true;
                Self::spawn_artist_top(id, self.bg_tx.clone());
            }
            BrowseRoute::Album { id, title } => {
                self.selected_playlist_id = None;
                self.loading_loved_tracks = false;
                self.active_category =
                    format!("{} • {}", text(self.language, "Albüm"), title).into();
                self.is_searching = true;
                Self::spawn_album_tracks(id, self.bg_tx.clone());
            }
        }
    }

    pub fn toggle_discord_rpc(&mut self) {
        let next = !self.discord_rpc.is_enabled();
        self.discord_rpc.set_enabled(next);
        self.save_config();
    }

    pub fn toggle_lastfm_enabled(&mut self) {
        let next = !self.lastfm.is_enabled();
        self.lastfm.set_enabled(next);
        self.save_config();
    }

    pub fn save_config(&self) {
        let cfg = AppConfig {
            volume: self.volume,
            exclusive_mode: self.share_mode == OutputShareMode::Exclusive,
            selected_device_id: self.selected_device_id.clone(),
            theme: self.theme_id,
            eq_enabled: self.eq_enabled,
            eq_preset_name: self.eq_preset_name.to_string(),
            eq_bands: self.eq_bands,
            eq_preamp_db: self.eq_preamp_db,
            language: self.language,
            audio_quality: self.audio_quality,
            normalize_volume: self.normalize_volume,
            discord_rpc_enabled: self.discord_rpc.is_enabled(),
            lastfm_enabled: self.lastfm.is_enabled(),
        };
        cfg.save();
    }

    pub fn sync_eq(&mut self) {
        self.sync_eq_live();
        self.save_config();
    }

    fn sync_eq_live(&mut self) {
        self.eq_preamp_db = clamp_preamp_db(self.eq_preamp_db);
        self.audio
            .set_eq(self.eq_enabled, self.eq_bands, self.eq_preamp_db);
        self.engine.set_eq(
            self.eq_enabled,
            self.eq_bands,
            self.eq_preamp_db,
            self.share_mode == OutputShareMode::Shared,
        );
    }

    pub fn apply_preset(&mut self, name: &'static str, bands: [f32; 10]) {
        self.eq_enabled = true;
        self.eq_preset_name = name.into();
        self.eq_bands = bands;
        self.sync_eq();
    }

    pub fn apply_saved_preset(&mut self, index: usize) {
        let Some(preset) = self.eq_presets.get(index).cloned() else {
            return;
        };
        self.eq_enabled = true;
        self.eq_preset_name = preset.name.into();
        self.eq_bands = preset.bands;
        self.eq_preamp_db = clamp_preamp_db(preset.preamp_db);
        self.sync_eq();
    }

    pub fn save_preset_from_input(&mut self) {
        let typed = self.eq_preset_input.trim().to_string();
        let name = if typed.is_empty() {
            format!(
                "{} {}",
                text(self.language, "Özel"),
                self.eq_presets.len() + 1
            )
        } else {
            typed
        };

        if let Some(existing) = self
            .eq_presets
            .iter_mut()
            .find(|preset| preset.name.eq_ignore_ascii_case(&name))
        {
            existing.bands = self.eq_bands;
            existing.preamp_db = self.eq_preamp_db;
        } else {
            self.eq_presets.push(EqPreset {
                name: name.clone(),
                bands: self.eq_bands,
                preamp_db: self.eq_preamp_db,
            });
        }

        self.eq_preset_name = name.clone().into();
        self.eq_preset_input.clear();
        self.preset_name_focused = false;
        save_eq_presets(&self.eq_presets);
        self.sync_eq();
        self.action_notice = Some((
            format!("{}: {name}", text(self.language, "Preset kaydedildi")),
            Instant::now(),
        ));
    }

    pub fn delete_saved_preset(&mut self, index: usize) {
        if index >= self.eq_presets.len() {
            return;
        }
        let removed = self.eq_presets.remove(index);
        if self.eq_preset_name.as_ref() == removed.name {
            self.eq_preset_name = "Custom".into();
        }
        save_eq_presets(&self.eq_presets);
    }

    fn toggle_mute(&mut self) {
        self.search_focused = false;
        if self.muted {
            self.muted = false;
            self.volume = self.volume_before_mute.clamp(0.0, 1.0);
        } else {
            self.volume_before_mute = self.volume;
            self.muted = true;
        }
        self.engine
            .set_volume(if self.muted { 0.0 } else { self.volume });
    }

    fn adjust_volume(&mut self, delta: f32) {
        self.search_focused = false;
        self.muted = false;
        self.volume = (self.volume + delta).clamp(0.0, 1.0);
        self.engine.set_volume(self.volume);
        self.save_config();
    }

    fn set_volume_from_mouse_x(&mut self, mouse_x: f32, slider_right: f32) {
        let slider_left = slider_right - VOLUME_SLIDER_WIDTH;
        let ratio = ((mouse_x - slider_left) / VOLUME_SLIDER_WIDTH).clamp(0.0, 1.0);
        self.search_focused = false;
        self.muted = false;
        self.volume = ratio;
        self.engine.set_volume(self.volume);
    }

    fn toggle_playback(&mut self) {
        self.search_focused = false;
        self.engine.send_command("toggle");
    }

    fn volume_icon_name(&self) -> &'static str {
        if self.muted || self.volume <= 0.001 {
            "volume-x"
        } else if self.volume < 0.5 {
            "volume-1"
        } else {
            "volume-2"
        }
    }

    fn volume_label(&self) -> String {
        if self.muted {
            text(self.language, "Sessiz")
        } else {
            format!("{}%", (self.volume * 100.0).round() as u32)
        }
    }

    fn extract_dominant_color(path: &Path) -> Option<u32> {
        let img = image::open(path).ok()?.thumbnail(32, 32).to_rgb8();
        let mut r_sum = 0u64;
        let mut g_sum = 0u64;
        let mut b_sum = 0u64;
        let mut weight_sum = 0u64;

        for p in img.pixels() {
            let [r, g, b] = p.0;
            let max_c = r.max(g).max(b) as u64;
            let min_c = r.min(g).min(b) as u64;
            let sat = max_c.saturating_sub(min_c);
            let w = 1 + (sat * sat) / 64;
            r_sum += r as u64 * w;
            g_sum += g as u64 * w;
            b_sum += b as u64 * w;
            weight_sum += w;
        }

        if weight_sum == 0 {
            return None;
        }
        let r = ((r_sum / weight_sum) as u32).min(255);
        let g = ((g_sum / weight_sum) as u32).min(255);
        let b = ((b_sum / weight_sum) as u32).min(255);
        Some((r << 16) | (g << 8) | b)
    }

    fn rgba_with_alpha(rgb_hex: u32, alpha: u8) -> Rgba {
        rgba((rgb_hex << 8) | (alpha as u32))
    }

    fn fetch_paginated_tracks(initial_url: String, max_pages: usize) -> Vec<DeezerTrackItem> {
        let mut all = Vec::new();
        let mut next_url = Some(initial_url);
        let mut pages = 0;

        while let Some(url) = next_url {
            if pages >= max_pages {
                break;
            }
            pages += 1;
            if let Ok(resp) = reqwest::blocking::get(&url) {
                if let Ok(parsed) = resp.json::<DeezerSearchResponse>() {
                    all.extend(parsed.data);
                    next_url = parsed.next;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        all
    }

    fn fetch_search_page(url: &str) -> (Vec<DeezerTrackItem>, Option<String>) {
        match reqwest::blocking::get(url) {
            Ok(response) => match response.json::<DeezerSearchResponse>() {
                Ok(page) => (page.data, page.next),
                Err(_) => (Vec::new(), None),
            },
            Err(_) => (Vec::new(), None),
        }
    }

    fn push_unique_tracks(target: &mut Vec<DeezerTrackItem>, incoming: Vec<DeezerTrackItem>) {
        let mut known: HashSet<u64> = target.iter().map(|track| track.id).collect();
        for track in incoming {
            if known.insert(track.id) {
                target.push(track);
            }
        }
    }

    fn spawn_search(query: &str, tx: Sender<BgTaskResult>) {
        let q = query.trim().to_string();
        if q.is_empty() {
            return;
        }
        thread::spawn(move || {
            let encoded: String = q
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                        (b as char).to_string()
                    }
                    b' ' => "+".to_string(),
                    _ => format!("%{:02X}", b),
                })
                .collect();

            let url = format!("https://api.deezer.com/search?q={encoded}&limit=50");
            let (items, next) = Self::fetch_search_page(&url);
            let _ = tx.send(BgTaskResult::SearchPage(items, next));
        });
    }

    fn spawn_search_page(url: String, tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let (items, next) = Self::fetch_search_page(&url);
            let _ = tx.send(BgTaskResult::SearchPage(items, next));
        });
    }

    fn spawn_playlist_tracks(playlist_id: u64, tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let url = format!("https://api.deezer.com/playlist/{playlist_id}/tracks?limit=100");
            let items = Self::fetch_paginated_tracks(url, 10);
            let _ = tx.send(BgTaskResult::SearchResults(items));
        });
    }

    fn spawn_loved_tracks(user_id: u64, tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let url = format!("https://api.deezer.com/user/{user_id}/tracks?limit=100");
            let items = Self::fetch_paginated_tracks(url, 10);
            let _ = tx.send(BgTaskResult::SearchResults(items));
        });
    }

    fn spawn_loved_track_ids(user_id: u64, tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let client = reqwest::blocking::Client::new();
            let mut next = Some(format!("https://api.deezer.com/user/{user_id}/tracks?limit=100"));
            let mut ids = HashSet::new();
            while let Some(url) = next.take() {
                let Ok(response) = client.get(url).send() else { break };
                let Ok(page) = response.json::<DeezerSearchResponse>() else { break };
                ids.extend(page.data.into_iter().map(|track| track.id));
                next = page.next;
                if ids.len() >= 10_000 {
                    break;
                }
            }
            let _ = tx.send(BgTaskResult::LovedTrackIds(ids.into_iter().collect()));
        });
    }

    fn spawn_artist_top(artist_id: u64, tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let url = format!("https://api.deezer.com/artist/{artist_id}/top?limit=50");
            let items = Self::fetch_paginated_tracks(url, 1);
            let _ = tx.send(BgTaskResult::SearchResults(items));
        });
    }

    fn spawn_album_tracks(album_id: u64, tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let url = format!("https://api.deezer.com/album/{album_id}");
            if let Ok(resp) = reqwest::blocking::get(&url)
                && let Ok(album) = resp.json::<DeezerAlbumDetailResponse>()
            {
                let mut items = album.tracks.map(|t| t.data).unwrap_or_default();
                for item in &mut items {
                    if item.album.id == 0 {
                        item.album.id = album_id;
                    }
                    if item.album.title.is_empty() {
                        item.album.title = album.title.clone();
                    }
                    if item.album.cover_medium.is_empty() {
                        item.album.cover_medium = album.cover_medium.clone();
                    }
                }
                let _ = tx.send(BgTaskResult::SearchResults(items));
                return;
            }
            let _ = tx.send(BgTaskResult::SearchResults(Vec::new()));
        });
    }

    fn spawn_artwork_download(url: String, tx: Sender<BgTaskResult>) {
        thread::spawn(move || {
            let mut hasher = DefaultHasher::new();
            url.hash(&mut hasher);
            let file_name = format!("art_{:x}.jpg", hasher.finish());

            let cache_dir = data_dir().join("ArtworkCache");
            let _ = fs::create_dir_all(&cache_dir);
            let target_path = cache_dir.join(file_name);

            if target_path.exists() {
                let dom = Self::extract_dominant_color(&target_path);
                let _ = tx.send(BgTaskResult::ArtworkCached(url, target_path, dom));
                return;
            }

            if let Ok(resp) = reqwest::blocking::get(&url)
                && let Ok(bytes) = resp.bytes()
                && fs::write(&target_path, &bytes).is_ok()
            {
                let dom = Self::extract_dominant_color(&target_path);
                let _ = tx.send(BgTaskResult::ArtworkCached(url, target_path, dom));
            }
        });
    }

    fn submit_search(&mut self) {
        self.search_focused = false;
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.record_search(&query);
        self.apply_route(BrowseRoute::Search { query }, true);
    }

    fn record_search(&mut self, query: &str) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.search_history
            .retain(|item| !item.eq_ignore_ascii_case(&query));
        self.search_history.insert(0, query);
        self.search_history.truncate(SEARCH_HISTORY_LIMIT);
        save_search_history(&self.search_history);
    }

    fn use_history_query(&mut self, query: String) {
        self.search_query = query;
        self.search_focused = false;
        self.submit_search();
    }

    fn load_more_search(&mut self) {
        if self.loading_more {
            return;
        }
        let Some(next_url) = self.next_page_url.take() else {
            return;
        };
        self.loading_more = true;
        Self::spawn_search_page(next_url, self.bg_tx.clone());
    }

    fn trigger_search(&mut self, query: &str, category_label: &str) {
        self.apply_route(
            BrowseRoute::Category {
                query: query.to_string(),
                label: category_label.to_string(),
            },
            true,
        );
    }

    fn open_loved_tracks(&mut self) {
        self.apply_route(BrowseRoute::LovedTracks, true);
    }

    fn open_playlist(&mut self, pl: &UserPlaylist) {
        self.apply_route(
            BrowseRoute::Playlist {
                id: pl.id,
                title: pl.title.clone(),
            },
            true,
        );
    }

    fn toggle_search_track_favorite(&mut self, track_id: u64) {
        self.search_focused = false;
        if self.deezer_user_id.is_none() {
            self.action_notice = Some((
                text(self.language, "Beğenilenlere eklemek için önce Deezer oturumu aç."),
                Instant::now(),
            ));
            if !self.login_window_open {
                self.login_window_open = self.engine.toggle_login_window();
            }
            return;
        }
        if self.liked_track_ids.contains(&track_id) {
            self.engine.remove_track_from_favorites(track_id);
        } else {
            self.engine.add_track_to_favorites(track_id);
        }
    }

    fn remove_track_from_current_playlist(&mut self, track_id: u64) {
        self.search_focused = false;
        let Some(playlist_id) = self.selected_playlist_id else {
            return;
        };
        if self.deezer_user_id.is_none() {
            self.action_notice = Some((
                text(self.language, "Çalma listesine eklemek için önce Deezer oturumu aç."),
                Instant::now(),
            ));
            if !self.login_window_open {
                self.login_window_open = self.engine.toggle_login_window();
            }
            return;
        }
        self.engine.remove_track_from_playlist(track_id, playlist_id);
    }

    fn open_track_playlist_picker(&mut self, track_id: u64) {
        self.search_focused = false;
        if self.deezer_user_id.is_none() {
            self.action_notice = Some((
                text(self.language, "Çalma listesine eklemek için önce Deezer oturumu aç."),
                Instant::now(),
            ));
            if !self.login_window_open {
                self.login_window_open = self.engine.toggle_login_window();
            }
            return;
        }
        self.engine.refresh_playlists();
        self.playlist_picker_track = Some(track_id);
    }

    fn add_track_to_selected_playlist(&mut self, playlist_id: u64) {
        if let Some(track_id) = self.playlist_picker_track.take() {
            self.engine.add_track_to_playlist(track_id, playlist_id);
        }
    }

    fn open_new_playlist_modal(&mut self, track_id: Option<u64>) {
        self.search_focused = false;
        self.new_playlist_open = true;
        self.target_track_id = 0;
        self.new_playlist_track = track_id;
        self.new_playlist_input.clear();
        self.new_playlist_error = None;
    }

    fn close_new_playlist_modal(&mut self) {
        self.new_playlist_open = false;
        self.new_playlist_track = None;
        self.new_playlist_input.clear();
        self.new_playlist_error = None;
        self.pending_playlist_title = None;
        self.pending_playlist_since = None;
    }

    fn verify_pending_playlist(&mut self) {
        let Some(title) = self.pending_playlist_title.clone() else {
            return;
        };
        let matched = self
            .user_playlists
            .iter()
            .any(|pl| pl.title.trim().eq_ignore_ascii_case(title.trim()));
        if !matched {
            return;
        }
        self.pending_playlist_title = None;
        self.pending_playlist_since = None;
        self.new_playlist_open = false;
        self.new_playlist_error = None;
        self.new_playlist_track = None;
        self.new_playlist_input.clear();
        self.action_notice = Some((
            text(self.language, "Çalma listesi oluşturuldu."),
            Instant::now(),
        ));
    }

    fn check_pending_playlist_timeout(&mut self) {
        let Some(since) = self.pending_playlist_since else {
            return;
        };
        if since.elapsed() < PLAYLIST_VERIFY_TIMEOUT {
            return;
        }
        let title = self.pending_playlist_title.take();
        self.pending_playlist_since = None;
        if let Some(title) = title
            && self
                .user_playlists
                .iter()
                .any(|pl| pl.title.trim().eq_ignore_ascii_case(title.trim()))
        {
            self.new_playlist_input.clear();
            self.new_playlist_error = None;
            self.new_playlist_track = None;
            self.action_notice = Some((
                text(self.language, "Çalma listesi oluşturuldu."),
                Instant::now(),
            ));
            return;
        }
        self.new_playlist_open = true;
        self.search_focused = false;
        if self.new_playlist_error.is_none() {
            self.new_playlist_error =
                Some(text(self.language, "Çalma listesi oluşturulamadı."));
        }
    }

    fn request_delete_playlist(&mut self, playlist_id: u64) {
        self.search_focused = false;
        if matches!(self.delete_confirm_playlist, Some((id, _)) if id == playlist_id) {
            self.delete_confirm_playlist = None;
            self.engine.delete_playlist(playlist_id);
            self.action_notice = Some((
                text(self.language, "Çalma listesi siliniyor..."),
                Instant::now(),
            ));
        } else {
            self.delete_confirm_playlist = Some((playlist_id, Instant::now()));
        }
    }

    fn submit_new_playlist(&mut self) {
        let title = self.new_playlist_input.trim().to_string();
        if title.is_empty() {
            self.new_playlist_error = Some(text(self.language, "Çalma listesi adı boş olamaz."));
            return;
        }
        if self.deezer_user_id.is_none() {
            self.new_playlist_error = Some(text(
                self.language,
                "Çalma listesine eklemek için önce Deezer oturumu aç.",
            ));
            if !self.login_window_open {
                self.login_window_open = self.engine.toggle_login_window();
            }
            return;
        }
        self.new_playlist_error = None;
        self.new_playlist_open = false;
        self.pending_playlist_title = Some(title.clone());
        self.pending_playlist_since = Some(Instant::now());
        let track = self.new_playlist_track;
        self.engine.create_playlist(title, track);
        self.action_notice = Some((
            text(self.language, "Çalma listesi oluşturuluyor..."),
            Instant::now(),
        ));
    }

    fn render_new_playlist_modal(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.language;
        let input_empty = self.new_playlist_input.is_empty();
        let input_display = if input_empty {
            text(language, "Çalma listesi adı")
        } else {
            format!("{}|", self.new_playlist_input)
        };
        let error = self.new_playlist_error.clone();
        let with_track = self.new_playlist_track.is_some();
        let can_submit = !input_empty;

        div()
            .id("new-playlist-backdrop")
            .absolute()
            .inset_0()
            .bg(rgba(0x040408A8))
            .flex()
            .items_center()
            .justify_center()
            .p(px(20.0))
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_new_playlist_modal();
                cx.notify();
            }))
            .child(
                div()
                    .id("new-playlist-card")
                    .w(px(420.0))
                    .p(px(16.0))
                    .rounded(px(12.0))
                    .bg(rgb(pal.modal_bg))
                    .border_1()
                    .border_color(rgb(pal.border_strong))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(pal.text_primary))
                                    .child(text(language, "Yeni çalma listesi")),
                            )
                            .child(
                                div()
                                    .id("close-new-playlist")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded(px(5.0))
                                    .bg(rgb(pal.bg_chip))
                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(pal.text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_new_playlist_modal();
                                        cx.notify();
                                    }))
                                    .child(lucide_icon("x", 13.0, rgb(pal.text_secondary)))
                                    .child(text(language, "Kapat")),
                            ),
                    )
                    .child(
                        div()
                            .id("new-playlist-input")
                            .px(px(10.0))
                            .py(px(9.0))
                            .rounded(px(7.0))
                            .bg(rgb(pal.bg_row))
                            .border_1()
                            .border_color(rgb(pal.accent))
                            .text_size(px(12.5))
                            .cursor_text()
                            .text_color(if input_empty {
                                rgb(pal.text_muted)
                            } else {
                                rgb(pal.text_primary)
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.search_focused = false;
                                window.focus(&this.focus_handle, cx);
                                cx.notify();
                            }))
                            .child(input_display),
                    )
                    .when_some(error, |card, message| {
                        card.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(0xE5484D))
                                .child(message),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(pal.text_muted))
                            .child(if with_track {
                                text(language, "Liste oluşturulunca çalan şarkı eklenecek. Enter ile onayla, Esc ile kapat.")
                            } else {
                                text(language, "Enter ile oluştur, Esc ile kapat.")
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .id("cancel-new-playlist")
                                    .px(px(10.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(pal.bg_chip))
                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                    .cursor_pointer()
                                    .text_size(px(11.5))
                                    .text_color(rgb(pal.text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_new_playlist_modal();
                                        cx.notify();
                                    }))
                                    .child(text(language, "İptal")),
                            )
                            .child(
                                div()
                                    .id("confirm-new-playlist")
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(if can_submit {
                                        rgb(pal.accent)
                                    } else {
                                        rgb(pal.bg_chip)
                                    })
                                    .cursor_pointer()
                                    .text_size(px(11.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(if can_submit {
                                        rgb(pal.bg_root)
                                    } else {
                                        rgb(pal.text_muted)
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.submit_new_playlist();
                                        cx.notify();
                                    }))
                                    .child(text(language, "Oluştur")),
                            ),
                    ),
            )
    }

    fn render_playlist_picker(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let playlists = self.user_playlists.clone();
        let language = self.language;
        div()
            .id("playlist-picker-backdrop")
            .absolute()
            .inset_0()
            .bg(rgba(0x040408A8))
            .flex()
            .items_center()
            .justify_center()
            .p(px(20.0))
            .on_click(cx.listener(|this, _, _, cx| {
                this.playlist_picker_track = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("playlist-picker-card")
                    .w(px(390.0))
                    .max_h(px(460.0))
                    .p(px(16.0))
                    .rounded(px(12.0))
                    .bg(rgb(pal.modal_bg))
                    .border_1()
                    .border_color(rgb(pal.border_strong))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(pal.text_primary))
                                    .child(text(self.language, "Şarkıyı çalma listesine ekle")),
                            )
                            .child(
                                div()
                                    .id("close-playlist-picker")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded(px(5.0))
                                    .bg(rgb(pal.bg_chip))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(pal.text_secondary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.playlist_picker_track = None;
                                        cx.notify();
                                    }))
                                    .child(lucide_icon("x", 13.0, rgb(pal.text_secondary)))
                                    .child(text(self.language, "Kapat")),
                            ),
                    )
                    .when(playlists.is_empty(), |card| {
                        card.child(
                            div()
                                .p(px(10.0))
                                .text_size(px(12.0))
                                .text_color(rgb(pal.text_secondary))
                                .child(text(
                                    self.language,
                                    "Çalma listesi bulunamadı. Deezer oturumunu açıp tekrar dene.",
                                )),
                        )
                    })
                    .when(!playlists.is_empty(), |card| {
                        card.child(
                            div()
                                .id("playlist-picker-options")
                                .max_h(px(350.0))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap(px(5.0))
                                .children(playlists.into_iter().map(|playlist| {
                                    let id = playlist.id;
                                    div()
                                        .id(ElementId::Name(format!("add-to-playlist-{id}").into()))
                                        .px(px(10.0))
                                        .py(px(9.0))
                                        .rounded(px(7.0))
                                        .bg(rgb(pal.bg_row))
                                        .hover(|s| s.bg(rgb(pal.bg_row_hover)))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .text_size(px(12.0))
                                        .text_color(rgb(pal.text_primary))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.add_track_to_selected_playlist(id);
                                            cx.notify();
                                        }))
                                        .child(lucide_icon(
                                            "list-music",
                                            14.0,
                                            rgb(pal.text_secondary),
                                        ))
                                        .child(playlist.title)
                                })),
                        )
                    })
                    .when_some(self.playlist_picker_track, move |card, track_id| {
                        card.child(
                            div()
                                .id("create-playlist-from-picker")
                                .px(px(10.0))
                                .py(px(9.0))
                                .rounded(px(7.0))
                                .bg(rgb(pal.bg_chip))
                                .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .text_size(px(12.0))
                                .text_color(rgb(pal.accent_bright))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.playlist_picker_track = None;
                                    this.open_new_playlist_modal(Some(track_id));
                                    cx.notify();
                                }))
                                .child(lucide_icon("plus", 14.0, rgb(pal.accent_bright)))
                                .child(text(language, "Yeni çalma listesi oluştur")),
                        )
                    }),
            )
    }

    fn open_artist(&mut self, artist: &DeezerArtistInfo) {
        self.search_focused = false;
        if artist.id == 0 {
            return;
        }
        self.apply_route(
            BrowseRoute::Artist {
                id: artist.id,
                name: artist.name.clone(),
            },
            true,
        );
    }

    fn open_album(&mut self, album: &DeezerAlbumInfo) {
        self.search_focused = false;
        if album.id == 0 {
            return;
        }
        self.apply_route(
            BrowseRoute::Album {
                id: album.id,
                title: album.title.clone(),
            },
            true,
        );
    }

    fn select_track_optimistic(&mut self, track: &DeezerTrackItem) {
        self.target_track_id = track.id;
        self.state.track_id = track.id;
        self.state.title = track.title.clone();
        self.state.artist = track.artist.name.clone();
        self.state.duration = track.duration as f64;
        self.state.current_time = 0.0;
        self.state.is_playing = true;
        self.last_track_advance = Instant::now();
        self.last_seek_at = Instant::now();
        self.audio.notify_playback_started();

        if !track.album.cover_medium.is_empty() {
            if track.album.cover_medium != self.cached_artwork_url {
                self.cached_artwork_url = track.album.cover_medium.clone();
                self.cached_artwork_path = self.thumb_cache.get(&track.album.cover_medium).cloned();
                Self::spawn_artwork_download(track.album.cover_medium.clone(), self.bg_tx.clone());
            }
        } else {
            self.cached_artwork_url.clear();
            self.cached_artwork_path = None;
            self.cover_ambient_rgb = None;
        }
    }

    fn start_from_current_list(&mut self, start_idx: usize, shuffle: bool) {
        self.search_focused = false;
        if self.tracks.is_empty() {
            return;
        }
        let mut list = self.tracks.clone();
        let chosen_id = list.get(start_idx).map(|t| t.id).unwrap_or(list[0].id);

        if shuffle {
            self.shuffle_enabled = true;
            Self::shuffle_vec(&mut list, chosen_id);
        }

        let new_idx = list.iter().position(|t| t.id == chosen_id).unwrap_or(0);
        let track = list[new_idx].clone();
        self.queue = list;
        self.queue_index = Some(new_idx);
        self.select_track_optimistic(&track);
        self.sync_eq_live();
        self.engine
            .set_volume(if self.muted { 0.0 } else { self.volume });
        self.engine.send_command("eq_now");
        self.engine.play_track_id(track.id);
        self.pending_eq_reapply_until = Some(Instant::now() + Duration::from_secs(4));
    }

    fn play_queue_index(&mut self, idx: usize) {
        self.search_focused = false;
        if let Some(track) = self.queue.get(idx).cloned() {
            self.queue_index = Some(idx);
            self.select_track_optimistic(&track);
            self.sync_eq_live();
            self.engine
                .set_volume(if self.muted { 0.0 } else { self.volume });
            self.engine.send_command("eq_now");
            self.engine.play_track_id(track.id);
            self.pending_eq_reapply_until = Some(Instant::now() + Duration::from_secs(4));
        }
    }

    #[allow(dead_code)]
    fn _nocturne_eq_now_only(&self) {
        let _ = self.engine.send_command("eq_now");
    }

    fn toggle_shuffle(&mut self) {
        self.search_focused = false;
        self.shuffle_enabled = !self.shuffle_enabled;
        if self.shuffle_enabled && self.queue.len() > 1 {
            let current_id = self
                .queue_index
                .and_then(|i| self.queue.get(i))
                .map(|t| t.id)
                .unwrap_or(0);
            Self::shuffle_vec(&mut self.queue, current_id);
            self.queue_index = self.queue.iter().position(|t| t.id == current_id);
        }
    }

    fn shuffle_vec(list: &mut [DeezerTrackItem], keep_first_id: u64) {
        if list.len() <= 1 {
            return;
        }
        let mut seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);

        for i in (1..list.len()).rev() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let j = (seed as usize) % (i + 1);
            list.swap(i, j);
        }

        if keep_first_id != 0
            && let Some(pos) = list.iter().position(|t| t.id == keep_first_id)
        {
            list.swap(0, pos);
        }
    }

    fn play_next(&mut self) {
        self.search_focused = false;
        if self.queue.is_empty() {
            self.state.is_playing = false;
            self.engine.send_command("stop");
            return;
        }
        let current = self.queue_index.unwrap_or(0);
        if current + 1 < self.queue.len() {
            self.play_queue_index(current + 1);
        } else if self.repeat_mode == RepeatMode::All {
            self.play_queue_index(0);
        } else {
            self.state.is_playing = false;
            self.state.current_time = 0.0;
            self.engine.send_command("stop");
        }
    }

    fn play_prev(&mut self) {
        self.search_focused = false;
        if self.state.current_time > 3.0 {
            self.state.current_time = 0.0;
            self.last_seek_at = Instant::now();
            self.engine.seek_to(0.0);
            return;
        }
        if self.queue.is_empty() {
            self.engine.send_command("prev");
            return;
        }
        let current = self.queue_index.unwrap_or(0);
        if current > 0 {
            self.play_queue_index(current - 1);
        } else if self.repeat_mode == RepeatMode::All {
            let last = self.queue.len().saturating_sub(1);
            self.play_queue_index(last);
        } else {
            self.state.current_time = 0.0;
            self.last_seek_at = Instant::now();
            self.engine.seek_to(0.0);
        }
    }

    fn handle_track_ended(&mut self) {
        self.last_track_advance = Instant::now();
        self.last_seek_at = Instant::now();
        if self.repeat_mode == RepeatMode::One {
            self.state.current_time = 0.0;
            self.state.is_playing = true;
            self.engine.send_command("restart");
            return;
        }
        self.play_next();
    }

    fn stop_playback(&mut self) {
        self.state.is_playing = false;
        self.state.current_time = 0.0;
        self.target_track_id = 0;
        self.last_track_advance = Instant::now();
        self.discord_rpc.clear();
        self.lastfm.on_playback_state(
            self.state.track_id,
            &self.state.title,
            &self.state.artist,
            self.state.duration,
            false,
            false,
        );
        self.engine.send_command("stop");
    }

    fn remove_queue_item(&mut self, index: usize) {
        if index >= self.queue.len() {
            return;
        }
        self.search_focused = false;
        let was_current = self.queue_index == Some(index);
        self.queue.remove(index);

        self.queue_index = match self.queue_index {
            Some(current) if current > index => Some(current - 1),
            _ if was_current => {
                if self.queue.is_empty() {
                    None
                } else {
                    Some(index.min(self.queue.len() - 1))
                }
            }
            other => other,
        };

        if was_current && self.queue.is_empty() {
            self.stop_playback();
        }
    }

    fn clear_queue(&mut self) {
        self.search_focused = false;
        if self.queue.is_empty() {
            return;
        }
        self.queue.clear();
        self.queue_index = None;
        self.stop_playback();
    }

    fn format_time(sec: f64) -> String {
        if !sec.is_finite() || sec <= 0.0 {
            return "0:00".to_string();
        }
        let total = sec as u64;
        format!("{}:{:02}", total / 60, total % 60)
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let typed_char = event.keystroke.key_char.as_deref();

        if self.new_playlist_open {
            if modifiers.control && key.eq_ignore_ascii_case("v") {
                if let Some(item) = cx.read_from_clipboard()
                    && let Some(text) = item.text()
                {
                    let cleaned: String = text
                        .chars()
                        .map(|c| if matches!(c, '\r' | '\n' | '\t') { ' ' } else { c })
                        .collect();
                    let cleaned = cleaned.trim();
                    if !cleaned.is_empty() {
                        self.new_playlist_input.push_str(cleaned);
                        self.new_playlist_error = None;
                    }
                }
            } else if key == "escape" {
                self.close_new_playlist_modal();
            } else if key == "backspace" {
                self.new_playlist_input.pop();
                self.new_playlist_error = None;
            } else if key == "enter" {
                self.submit_new_playlist();
            } else if key == "space" {
                self.new_playlist_input.push(' ');
                self.new_playlist_error = None;
            } else if !modifiers.control
                && !modifiers.alt
                && let Some(ch) = typed_char
            {
                self.new_playlist_input.push_str(ch);
                self.new_playlist_error = None;
            }
            cx.notify();
            return;
        }

        if self.settings_modal_open {
            if key == "escape" {
                self.settings_modal_open = false;
                self.preset_name_focused = false;
                cx.notify();
                return;
            }
            if self.preset_name_focused && self.settings_tab == SettingsTab::Equalizer {
                if key == "backspace" {
                    self.eq_preset_input.pop();
                } else if key == "enter" {
                    self.save_preset_from_input();
                } else if key == "space" {
                    self.eq_preset_input.push(' ');
                } else if !modifiers.control
                    && !modifiers.alt
                    && let Some(ch) = typed_char
                {
                    self.eq_preset_input.push_str(ch);
                }
                cx.notify();
            }
            return;
        }

        if modifiers.alt && matches!(key, "left" | "arrowleft") {
            self.navigate_back();
            cx.notify();
            return;
        }
        if modifiers.alt && matches!(key, "right" | "arrowright") {
            self.navigate_forward();
            cx.notify();
            return;
        }

        if modifiers.control && key == "f" {
            self.search_focused = true;
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }

        if modifiers.control && matches!(key, "left" | "arrowleft") {
            self.play_prev();
            cx.notify();
            return;
        }
        if modifiers.control && matches!(key, "right" | "arrowright") {
            self.play_next();
            cx.notify();
            return;
        }

        if self.search_focused {
            if modifiers.control && key.eq_ignore_ascii_case("v") {
                if let Some(item) = cx.read_from_clipboard()
                    && let Some(text) = item.text()
                {
                    let cleaned: String = text
                        .chars()
                        .map(|c| if matches!(c, '\r' | '\n' | '\t') { ' ' } else { c })
                        .collect();
                    let cleaned = cleaned.trim();
                    if !cleaned.is_empty() {
                        if !self.search_query.is_empty() && !self.search_query.ends_with(' ') {
                            self.search_query.push(' ');
                        }
                        self.search_query.push_str(cleaned);
                    }
                    cx.notify();
                }
            } else if modifiers.control && key.eq_ignore_ascii_case("c") {
                if !self.search_query.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        self.search_query.clone(),
                    ));
                }
            } else if modifiers.control && key.eq_ignore_ascii_case("x") {
                if !self.search_query.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        self.search_query.clone(),
                    ));
                    self.search_query.clear();
                    cx.notify();
                }
            } else if modifiers.control && key == "backspace" {
                let trimmed = self.search_query.trim_end().len();
                self.search_query.truncate(trimmed);
                if let Some(pos) = self.search_query.rfind(' ') {
                    self.search_query.truncate(pos + 1);
                } else {
                    self.search_query.clear();
                }
                cx.notify();
            } else if key == "escape" {
                self.search_focused = false;
                cx.notify();
            } else if key == "backspace" {
                self.search_query.pop();
                cx.notify();
            } else if key == "enter" {
                self.search_focused = false;
                self.submit_search();
                cx.notify();
            } else if key == "space" {
                self.search_query.push(' ');
                cx.notify();
            } else if !modifiers.control
                && !modifiers.alt
                && let Some(ch) = typed_char
            {
                self.search_query.push_str(ch);
                cx.notify();
            }
            return;
        }

        let plus = matches!(key, "+" | "add" | "num_add") || typed_char == Some("+");
        let minus = matches!(key, "-" | "subtract" | "num_sub") || typed_char == Some("-");

        if key == "space" {
            self.toggle_playback();
            cx.notify();
        } else if key == "m" && !modifiers.control && !modifiers.alt {
            self.toggle_mute();
            cx.notify();
        } else if plus && !modifiers.control {
            self.adjust_volume(0.05);
            cx.notify();
        } else if minus && !modifiers.control {
            self.adjust_volume(-0.05);
            cx.notify();
        }
    }

    fn nav_item(
        item: &'static NavItemDef,
        active_cat: &SharedString,
        pal: ThemePalette,
        language: LanguageSetting,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let localized_category = if let Some((_, label)) = item.cat_title.split_once(" • ") {
            format!("{} • {}", text(language, "Keşfet"), text(language, label))
        } else {
            text(language, item.cat_title)
        };
        let is_active = active_cat.as_ref() == localized_category;
        let query = item.query;
        let label = item.label;
        let text_col = rgb(if is_active {
            pal.accent_soft_text
        } else {
            pal.text_secondary
        });
        div()
            .id(item.id)
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(7.0))
            .bg(if is_active {
                rgb(pal.bg_row_active)
            } else {
                rgba(0x00000000)
            })
            .hover(|s| s.bg(rgb(pal.bg_row_hover)))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(12.5))
            .text_color(text_col)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.trigger_search(query, &localized_category);
                cx.notify();
            }))
            .child(lucide_icon("disc-3", 14.0, text_col))
            .child(text(language, label))
    }
}

impl Focusable for NocturneApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for NocturneApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = self.theme_id.palette();
        let ambient_hex = self.cover_ambient_rgb.unwrap_or(pal.accent);
        let can_back = self.can_navigate_back();
        let can_forward = self.can_navigate_forward();

        let progress_ratio = if self.state.duration > 0.0 {
            (self.state.current_time / self.state.duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };

        let title_text = if self.state.title.is_empty() {
            text(self.language, "Parça Seçilmedi")
        } else {
            self.state.title.clone()
        };

        let artist_text = if self.state.artist.is_empty() {
            text(self.language, "Listeden bir şarkıya tıkla veya arama yap")
        } else {
            self.state.artist.clone()
        };

        let search_display = if self.search_query.is_empty() {
            text(self.language, "Sanatçı, şarkı veya albüm ara (Enter)...")
        } else {
            format!("{}|", self.search_query)
        };

        let win_width: f32 = window.viewport_size().width.into();
        let seek_bar_width = (win_width - 148.0).max(200.0);
        let volume_slider_right = win_width - 24.0;
        let effective_volume = if self.muted {
            0.0
        } else {
            self.volume.clamp(0.0, 1.0)
        };
        let active_queue_id = self
            .queue_index
            .and_then(|i| self.queue.get(i))
            .map(|t| t.id)
            .unwrap_or(self.target_track_id);
        let in_playlist_view = self.selected_playlist_id.is_some();
        let loved_active = self.loading_loved_tracks
            || self
                .active_category
                .as_ref()
                .starts_with(&text(self.language, "Beğenilen Şarkılar"));

        div()
            .id("nocturne-root")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_move(cx.listener(move |this, evt: &MouseMoveEvent, _, cx| {
                if this.volume_dragging {
                    let move_x: f32 = evt.position.x.into();
                    this.set_volume_from_mouse_x(move_x, volume_slider_right);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.volume_dragging {
                        this.volume_dragging = false;
                        this.save_config();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, _, cx| {
                    this.navigate_back();
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, _, cx| {
                    this.navigate_forward();
                    cx.notify();
                }),
            )
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(pal.bg_root))
            .text_color(rgb(pal.text_primary))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(210.0))
                    .bg(Self::rgba_with_alpha(ambient_hex, 0x1E)),
            )
            // 1. ÜST AUDIOPHILE DURUM, TEMA VE ARAMA BARI
            .child(
                div()
                    .h(px(52.0))
                    .px(px(16.0))
                    .bg(Self::rgba_with_alpha(pal.bg_topbar, 0xE8))
                    .border_b_1()
                    .border_color(rgb(pal.border_subtle))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_size(px(14.5))
                                    .text_color(rgb(pal.accent_bright))
                                    .child("NOCTURNE"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .id("nav-history-back")
                                            .w(px(26.0))
                                            .h(px(26.0))
                                            .rounded(px(6.0))
                                            .bg(rgb(pal.bg_chip))
                                            .border_1()
                                            .border_color(if can_back {
                                                rgb(pal.border_strong)
                                            } else {
                                                rgb(pal.border_subtle)
                                            })
                                            .when(can_back, |btn| {
                                                btn.hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.navigate_back();
                                                        cx.notify();
                                                    }))
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(lucide_icon(
                                                "chevron-left",
                                                14.0,
                                                rgb(if can_back {
                                                    pal.text_primary
                                                } else {
                                                    pal.text_muted
                                                }),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("nav-history-forward")
                                            .w(px(26.0))
                                            .h(px(26.0))
                                            .rounded(px(6.0))
                                            .bg(rgb(pal.bg_chip))
                                            .border_1()
                                            .border_color(if can_forward {
                                                rgb(pal.border_strong)
                                            } else {
                                                rgb(pal.border_subtle)
                                            })
                                            .when(can_forward, |btn| {
                                                btn.hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.navigate_forward();
                                                        cx.notify();
                                                    }))
                                            })
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(lucide_icon(
                                                "chevron-right",
                                                14.0,
                                                rgb(if can_forward {
                                                    pal.text_primary
                                                } else {
                                                    pal.text_muted
                                                }),
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .id("status-badge-pill")
                                    .px(px(9.0))
                                    .py(px(3.5))
                                    .rounded(px(6.0))
                                    .bg(if self.audio_diag.exclusive_verified {
                                        rgb(0x113024)
                                    } else {
                                        rgb(pal.bg_chip)
                                    })
                                    .border_1()
                                    .border_color(if self.audio_diag.exclusive_verified {
                                        rgb(0x10B981)
                                    } else {
                                        rgb(pal.border_strong)
                                    })
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(11.0))
                                    .text_color(if self.audio_diag.exclusive_verified {
                                        rgb(0xA7F3D0)
                                    } else {
                                        rgb(pal.text_secondary)
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_focused = false;
                                        this.settings_modal_open = true;
                                        Self::spawn_device_scan(this.bg_tx.clone());
                                        cx.notify();
                                    }))
                                    .child(lucide_icon(
                                        "headphones",
                                        12.0,
                                        rgb(if self.audio_diag.exclusive_verified {
                                            0xA7F3D0
                                        } else {
                                            pal.text_secondary
                                        }),
                                    ))
                                    .child(self.audio_diag.badge.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id("search-box")
                            .flex_1()
                            .max_w(px(360.0))
                            .h(px(33.0))
                            .px(px(10.0))
                            .rounded(px(8.0))
                            .bg(rgb(pal.bg_panel))
                            .border_1()
                            .border_color(if self.search_focused {
                                rgb(pal.accent)
                            } else {
                                rgb(pal.border_strong)
                            })
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(7.0))
                            .cursor_text()
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.new_playlist_open {
                                    return;
                                }
                                this.search_focused = true;
                                window.focus(&this.focus_handle, cx);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.0))
                                    .overflow_hidden()
                                    .child(lucide_icon("search", 13.5, rgb(pal.text_muted)))
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(if self.search_query.is_empty() {
                                                rgb(pal.text_muted)
                                            } else {
                                                rgb(pal.text_primary)
                                            })
                                            .child(search_display),
                                    ),
                            )
                            .when(!self.search_query.is_empty(), |box_div| {
                                box_div.child(
                                    div()
                                        .id("clear-search-btn")
                                        .p(px(3.0))
                                        .rounded(px(4.0))
                                        .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.search_query.clear();
                                            cx.notify();
                                        }))
                                        .child(lucide_icon("x", 12.5, rgb(pal.text_secondary))),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .when_some(self.updater.available_update(), |bar, upd| {
                                let ver = upd.latest_version;
                                bar.child(
                                    div()
                                        .id("topbar-update-available")
                                        .px(px(10.0))
                                        .py(px(5.0))
                                        .rounded(px(6.0))
                                        .bg(rgb(0x166534))
                                        .border_1()
                                        .border_color(rgb(0x22C55E))
                                        .hover(|s| s.bg(rgb(0x15803D)))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .text_size(px(11.5))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xECFDF5))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.search_focused = false;
                                            this.updater.open_update_download();
                                            cx.notify();
                                        }))
                                        .child(lucide_icon("download", 13.5, rgb(0xECFDF5)))
                                        .child(format!("v{ver} İndir")),
                                )
                            })
                            .child(
                                div()
                                    .id("quick-theme-cycle")
                                    .px(px(10.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(pal.bg_chip))
                                    .border_1()
                                    .border_color(rgb(pal.border_strong))
                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(11.5))
                                    .text_color(rgb(pal.accent_bright))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_focused = false;
                                        this.theme_id = this.theme_id.next();
                                        this.save_config();
                                        cx.notify();
                                    }))
                                    .child(lucide_icon("palette", 13.5, rgb(pal.accent_bright)))
                                    .child(self.theme_id.label()),
                            )
                            .child(
                                div()
                                    .id("toggle-queue-drawer")
                                    .px(px(10.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .bg(if self.queue_drawer_open {
                                        rgb(pal.accent)
                                    } else {
                                        rgb(pal.bg_chip)
                                    })
                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(11.5))
                                    .text_color(rgb(pal.text_primary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_focused = false;
                                        this.queue_drawer_open = !this.queue_drawer_open;
                                        cx.notify();
                                    }))
                                    .child(lucide_icon("list-music", 13.5, rgb(pal.text_primary)))
                                    .child(format!(
                                        "{} ({})",
                                        text(self.language, "Sıra"),
                                        self.queue.len()
                                    )),
                            )
                            .child(
                                div()
                                    .id("open-unified-settings")
                                    .px(px(11.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .bg(if self.settings_modal_open {
                                        rgb(pal.accent)
                                    } else {
                                        rgb(pal.bg_chip)
                                    })
                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(11.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(pal.text_primary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_focused = false;
                                        this.settings_modal_open = !this.settings_modal_open;
                                        if this.settings_modal_open {
                                            Self::spawn_device_scan(this.bg_tx.clone());
                                        }
                                        cx.notify();
                                    }))
                                    .child(lucide_icon("settings", 13.5, rgb(pal.text_primary)))
                                    .child(text(self.language, "Ayarlar")),
                            )
                            .child(
                                div()
                                    .id("toggle-deezer-web")
                                    .px(px(11.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .bg(if self.login_window_open {
                                        rgb(pal.accent)
                                    } else {
                                        rgb(pal.bg_chip)
                                    })
                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(11.5))
                                    .text_color(rgb(pal.text_primary))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.search_focused = false;
                                        this.login_window_open =
                                            this.engine.toggle_login_window();
                                        cx.notify();
                                    }))
                                    .child(lucide_icon(
                                        if self.login_window_open {
                                            "log-out"
                                        } else {
                                            "circle-user-round"
                                        },
                                        13.5,
                                        rgb(pal.text_primary),
                                    ))
                                    .child(if self.login_window_open {
                                        text(self.language, "Oturumu Gizle")
                                    } else {
                                        text(self.language, "Deezer Oturumu")
                                    }),
                            ),
                    ),
            )
            // ARAMA GEÇMİŞİ HIZLI ERİŞİM
            .when(!self.search_history.is_empty(), |root| {
                root.child(
                    div()
                        .h(px(34.0))
                        .px(px(16.0))
                        .bg(Self::rgba_with_alpha(pal.bg_topbar, 0xE8))
                        .border_b_1()
                        .border_color(rgb(pal.border_subtle))
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .overflow_hidden()
                        .child(
                            div()
                                .text_size(px(10.5))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(rgb(pal.text_muted))
                                .child(text(self.language, "ARAMA GEÇMİŞİ")),
                        )
                        .children(self.search_history.iter().enumerate().map(
                            |(idx, entry)| {
                                let query = entry.clone();
                                let label = entry.clone();
                                div()
                                    .id(ElementId::Name(format!("search-history-{idx}").into()))
                                    .px(px(9.0))
                                    .py(px(3.5))
                                    .rounded(px(6.0))
                                    .bg(rgb(pal.bg_chip))
                                    .border_1()
                                    .border_color(rgb(pal.border_strong))
                                    .hover(|s| s.bg(rgb(pal.accent)))
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .text_color(rgb(pal.accent_soft_text))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.use_history_query(query.clone());
                                        cx.notify();
                                    }))
                                    .child(label)
                            },
                        )),
                )
            })
            // 2. ORTA ÇALIŞMA ALANI
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    // SOL KENAR ÇUBUĞU
                    .child(
                        div()
                            .id("left-sidebar-scroll")
                            .w(px(225.0))
                            .p(px(14.0))
                            .bg(Self::rgba_with_alpha(pal.bg_sidebar, 0xE6))
                            .border_r_1()
                            .border_color(rgb(pal.border_subtle))
                            .flex()
                            .flex_col()
                            .gap(px(5.0))
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(10.5))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(pal.text_muted))
                                    .child(text(self.language, "KİTAPLIĞIM")),
                            )
                            .child(
                                div()
                                    .id("nav-loved-tracks")
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .rounded(px(7.0))
                                    .bg(if loved_active {
                                        rgb(pal.bg_row_active)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .hover(|s| s.bg(rgb(pal.bg_row_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(pal.accent_bright))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_loved_tracks();
                                        cx.notify();
                                    }))
                                    .child(lucide_icon(
                                        "heart-filled",
                                        14.0,
                                        rgb(pal.accent_bright),
                                    ))
                                    .child(text(self.language, "Beğenilen Şarkılar")),
                            )
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .text_size(px(10.5))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(pal.text_muted))
                                    .child(text(self.language, "HIZLI KATEGORİLER")),
                            )
                            .child(Self::nav_item(
                                &NAV_PHONK,
                                &self.active_category,
                                pal,
                                self.language,
                                cx,
                            ))
                            .child(Self::nav_item(
                                &NAV_CAUCASIAN,
                                &self.active_category,
                                pal,
                                self.language,
                                cx,
                            ))
                            .child(Self::nav_item(
                                &NAV_DARKPOP,
                                &self.active_category,
                                pal,
                                self.language,
                                cx,
                            ))
                            .child(Self::nav_item(
                                &NAV_FLAC_TEST,
                                &self.active_category,
                                pal,
                                self.language,
                                cx,
                            ))
                            .child(Self::nav_item(
                                &NAV_TR_RAP,
                                &self.active_category,
                                pal,
                                self.language,
                                cx,
                            ))
                            .child(
                                div()
                                    .mt(px(12.0))
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(rgb(pal.text_muted))
                                            .child(text(self.language, "ÇALMA LİSTELERİM")),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .id("new-playlist-btn")
                                                    .px(px(6.0))
                                                    .py(px(2.5))
                                                    .rounded(px(4.0))
                                                    .bg(rgb(pal.bg_chip))
                                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                    .cursor_pointer()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(3.0))
                                                    .text_size(px(10.0))
                                                    .text_color(rgb(pal.accent_bright))
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.open_new_playlist_modal(None);
                                                        cx.notify();
                                                    }))
                                                    .child(lucide_icon(
                                                        "plus",
                                                        11.0,
                                                        rgb(pal.accent_bright),
                                                    ))
                                                    .child(text(self.language, "Yeni")),
                                            )
                                            .child(
                                                div()
                                                    .id("refresh-playlists-btn")
                                                    .p(px(4.0))
                                                    .rounded(px(4.0))
                                                    .bg(rgb(pal.bg_chip))
                                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                    .cursor_pointer()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .on_click(cx.listener(|this, _, _, _| {
                                                        this.engine.refresh_playlists();
                                                    }))
                                                    .child(lucide_icon(
                                                        "refresh-cw",
                                                        11.0,
                                                        rgb(pal.accent_bright),
                                                    )),
                                            ),
                                    ),
                            )
                            .children(self.user_playlists.iter().enumerate().map(|(i, pl)| {
                                let pl_clone = pl.clone();
                                let pl_id = pl.id;
                                let is_sel = self.selected_playlist_id == Some(pl.id);
                                let confirming_delete = matches!(self.delete_confirm_playlist, Some((id, _)) if id == pl.id);
                                let row_language = self.language;
                                let pl_color = rgb(if is_sel {
                                    pal.accent_soft_text
                                } else {
                                    pal.text_secondary
                                });
                                div()
                                    .id(ElementId::Name(format!("user-pl-{i}-{}", pl.id).into()))
                                    .px(px(10.0))
                                    .py(px(7.0))
                                    .rounded(px(7.0))
                                    .bg(if is_sel {
                                        rgb(pal.bg_row_active)
                                    } else {
                                        rgba(0x00000000)
                                    })
                                    .hover(|s| s.bg(rgb(pal.bg_row_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(6.0))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_playlist(&pl_clone);
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(7.0))
                                            .overflow_hidden()
                                            .child(lucide_icon("list-music", 13.0, pl_color))
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(pl_color)
                                                    .child(pl.title.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(5.0))
                                            .when(pl.nb_tracks > 0, |right| {
                                                right.child(
                                                    div()
                                                        .text_size(px(10.5))
                                                        .text_color(rgb(pal.text_muted))
                                                        .child(format!("{}", pl.nb_tracks)),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .id(ElementId::Name(
                                                        format!("delete-pl-{i}-{pl_id}").into(),
                                                    ))
                                                    .px(px(4.0))
                                                    .py(px(2.0))
                                                    .rounded(px(4.0))
                                                    .bg(if confirming_delete {
                                                        rgb(0x7A1F26)
                                                    } else {
                                                        rgba(0x00000000)
                                                    })
                                                    .hover(|s| s.bg(rgb(0x5A2128)))
                                                    .cursor_pointer()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_size(px(10.0))
                                                    .text_color(if confirming_delete {
                                                        rgb(0xFFB4B4)
                                                    } else {
                                                        rgb(pal.text_muted)
                                                    })
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        cx.stop_propagation();
                                                        this.request_delete_playlist(pl_id);
                                                        cx.notify();
                                                    }))
                                                    .when(confirming_delete, |btn| {
                                                        btn.child(text(row_language, "Sil?"))
                                                    })
                                                    .when(!confirming_delete, |btn| {
                                                        btn.child(lucide_icon(
                                                            "trash-2",
                                                            11.5,
                                                            rgb(pal.text_muted),
                                                        ))
                                                    }),
                                            ),
                                    )
                            })),
                    )
                    // ORTA ŞARKI LİSTESİ / DETAY GÖRÜNÜMÜ
                    .child(
                        div()
                            .flex_1()
                            .p(px(20.0))
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .text_size(px(19.0))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child(self.active_category.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .when(!self.tracks.is_empty(), |bar| {
                                                bar.child(
                                                    div()
                                                        .id("play-all-list-btn")
                                                        .px(px(13.0))
                                                        .py(px(6.0))
                                                        .rounded(px(7.0))
                                                        .bg(rgb(pal.accent))
                                                        .hover(|s| s.bg(rgb(pal.accent_bright)))
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.0))
                                                        .text_size(px(12.0))
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .text_color(rgb(pal.text_primary))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.start_from_current_list(0, false);
                                                            cx.notify();
                                                        }))
                                                        .child(lucide_icon(
                                                            "play-filled",
                                                            13.0,
                                                            rgb(pal.text_primary),
                                                        ))
                                                        .child(text(self.language, "Tümünü Çal")),
                                                )
                                                .child(
                                                    div()
                                                        .id("shuffle-all-list-btn")
                                                        .px(px(12.0))
                                                        .py(px(6.0))
                                                        .rounded(px(7.0))
                                                        .bg(rgb(pal.bg_chip))
                                                        .border_1()
                                                        .border_color(rgb(pal.border_strong))
                                                        .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.0))
                                                        .text_size(px(12.0))
                                                        .text_color(rgb(pal.accent_soft_text))
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.start_from_current_list(0, true);
                                                            cx.notify();
                                                        }))
                                                        .child(lucide_icon(
                                                            "shuffle",
                                                            13.0,
                                                            rgb(pal.accent_soft_text),
                                                        ))
                                                        .child(text(self.language, "Karışık Çal")),
                                                )
                                            })
                                            .when(self.is_searching, |d| {
                                                d.child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(rgb(pal.accent_bright))
                                                        .child(text(self.language, "Yükleniyor...")),
                                                )
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .id("track-list-scroll")
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.0))
                                    .overflow_y_scroll()
                                    .children(self.tracks.iter().enumerate().map(
                                        |(idx, track)| {
                                            let track_id = track.id;
                                            let t_title = track.title.clone();
                                            let artist_clone = track.artist.clone();
                                            let album_clone = track.album.clone();
                                            let t_artist = track.artist.name.clone();
                                            let t_album = track.album.title.clone();
                                            let t_dur = Self::format_time(track.duration as f64);
                                            let is_current = active_queue_id == track_id;
                                            let is_liked = self.liked_track_ids.contains(&track_id);
                                            let thumb_path = self
                                                .thumb_cache
                                                .get(&track.album.cover_medium)
                                                .cloned();

                                            div()
                                                .id(ElementId::Name(
                                                    format!("track-row-{idx}-{track_id}").into(),
                                                ))
                                                .px(px(13.0))
                                                .py(px(7.5))
                                                .rounded(px(9.0))
                                                .bg(if is_current {
                                                    rgb(pal.bg_row_active)
                                                } else {
                                                    Self::rgba_with_alpha(pal.bg_row, 0xD8)
                                                })
                                                .border_1()
                                                .border_color(if is_current {
                                                    rgb(pal.accent)
                                                } else {
                                                    rgb(pal.border_subtle)
                                                })
                                                .hover(|s| s.bg(rgb(pal.bg_row_hover)))
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_between()
                                                .gap(px(12.0))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.start_from_current_list(idx, false);
                                                    cx.notify();
                                                }))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(11.0))
                                                        .child(
                                                            div()
                                                                .w(px(22.0))
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .text_size(px(12.0))
                                                                .text_color(rgb(pal.text_muted))
                                                                .when(is_current, |cell| {
                                                                    cell.child(lucide_icon(
                                                                        "play-filled",
                                                                        13.0,
                                                                        rgb(pal.accent_bright),
                                                                    ))
                                                                })
                                                                .when(!is_current, |cell| {
                                                                    cell.child(format!(
                                                                        "{}",
                                                                        idx + 1
                                                                    ))
                                                                }),
                                                        )
                                                        .child(
                                                            div()
                                                                .w(px(38.0))
                                                                .h(px(38.0))
                                                                .rounded(px(6.0))
                                                                .bg(rgb(pal.bg_panel))
                                                                .border_1()
                                                                .border_color(if is_current {
                                                                    rgb(pal.accent)
                                                                } else {
                                                                    rgb(pal.border_subtle)
                                                                })
                                                                .overflow_hidden()
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .when_some(
                                                                    thumb_path,
                                                                    |cover_box, path| {
                                                                        cover_box.child(
                                                                            img(path).size_full(),
                                                                        )
                                                                    },
                                                                )
                                                                .when(
                                                                    !self.thumb_cache.contains_key(
                                                                        &track.album.cover_medium,
                                                                    ),
                                                                    |cover_box| {
                                                                        cover_box.child(
                                                                            lucide_icon(
                                                                                "music",
                                                                                15.0,
                                                                                rgb(pal.text_muted),
                                                                            ),
                                                                        )
                                                                    },
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .flex_col()
                                                                .gap(px(2.0))
                                                                .child(
                                                                    div()
                                                                        .text_size(px(13.0))
                                                                        .font_weight(
                                                                            gpui::FontWeight::SEMIBOLD,
                                                                        )
                                                                        .text_color(if is_current {
                                                                            rgb(pal.accent_soft_text)
                                                                        } else {
                                                                            rgb(pal.text_primary)
                                                                        })
                                                                        .child(t_title),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .items_center()
                                                                        .gap(px(6.0))
                                                                        .text_size(px(11.5))
                                                                        .text_color(rgb(
                                                                            pal.text_secondary,
                                                                        ))
                                                                        .child(
                                                                            div()
                                                                                .id(ElementId::Name(
                                                                                    format!(
                                                                                        "artist-link-{idx}-{track_id}"
                                                                                    )
                                                                                    .into(),
                                                                                ))
                                                                                .hover(|s| {
                                                                                    s.text_color(rgb(
                                                                                        pal.accent_bright,
                                                                                    ))
                                                                                })
                                                                                .on_click(cx.listener(
                                                                                    move |this, _, _, cx| {
                                                                                        cx.stop_propagation();
                                                                                        this.open_artist(&artist_clone);
                                                                                        cx.notify();
                                                                                    },
                                                                                ))
                                                                                .child(t_artist),
                                                                        )
                                                                        .when(
                                                                            !t_album.is_empty(),
                                                                            |sub| {
                                                                                sub.child("•").child(
                                                                                    div()
                                                                                        .id(ElementId::Name(
                                                                                            format!(
                                                                                                "album-link-{idx}-{track_id}"
                                                                                            )
                                                                                            .into(),
                                                                                        ))
                                                                                        .hover(|s| {
                                                                                            s.text_color(rgb(
                                                                                                pal.accent_bright,
                                                                                            ))
                                                                                        })
                                                                                        .on_click(cx.listener(
                                                                                            move |this, _, _, cx| {
                                                                                                cx.stop_propagation();
                                                                                                this.open_album(&album_clone);
                                                                                                cx.notify();
                                                                                            },
                                                                                        ))
                                                                                        .child(t_album),
                                                                                )
                                                                            },
                                                                        ),
                                                                ),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(8.0))
                                                        .child(
                                                            div()
                                                                .text_size(px(12.0))
                                                                .text_color(rgb(pal.text_muted))
                                                                .child(t_dur),
                                                        )
                                                        .child(
                                                            div()
                                                                .id(ElementId::Name(format!("favorite-track-{track_id}").into()))
                                                                .w(px(28.0))
                                                                .h(px(26.0))
                                                                .rounded(px(6.0))
                                                                .bg(rgb(if is_liked { pal.bg_row_active } else { pal.bg_chip }))
                                                                .border_1()
                                                                .border_color(if is_liked {
                                                                    rgb(pal.accent)
                                                                } else {
                                                                    rgba(0x00000000)
                                                                })
                                                                .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                                .cursor_pointer()
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    cx.stop_propagation();
                                                                    this.toggle_search_track_favorite(track_id);
                                                                    cx.notify();
                                                                }))
                                                                .child(lucide_icon(
                                                                    if is_liked {
                                                                        "heart-filled"
                                                                    } else {
                                                                        "heart"
                                                                    },
                                                                    14.0,
                                                                    rgb(if is_liked {
                                                                        pal.accent_bright
                                                                    } else {
                                                                        pal.accent_soft_text
                                                                    }),
                                                                )),
                                                        )
                                                        .child(
                                                            div()
                                                                .id(ElementId::Name(format!("playlist-add-track-{track_id}").into()))
                                                                .w(px(28.0))
                                                                .h(px(26.0))
                                                                .rounded(px(6.0))
                                                                .bg(rgb(pal.bg_chip))
                                                                .hover(|s| s.bg(rgb(pal.accent)))
                                                                .cursor_pointer()
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    cx.stop_propagation();
                                                                    this.open_track_playlist_picker(track_id);
                                                                    cx.notify();
                                                                }))
                                                                .child(lucide_icon(
                                                                    "list-plus",
                                                                    14.0,
                                                                    rgb(pal.accent_soft_text),
                                                                )),
                                                        )
                                                        .when(in_playlist_view, |actions| {
                                                            actions.child(
                                                                div()
                                                                    .id(ElementId::Name(
                                                                        format!(
                                                                            "playlist-remove-track-{idx}-{track_id}"
                                                                        )
                                                                        .into(),
                                                                    ))
                                                                    .w(px(28.0))
                                                                    .h(px(26.0))
                                                                    .rounded(px(6.0))
                                                                    .bg(rgb(pal.bg_chip))
                                                                    .hover(|s| s.bg(rgb(0x7A1F26)))
                                                                    .cursor_pointer()
                                                                    .flex()
                                                                    .items_center()
                                                                    .justify_center()
                                                                    .on_click(cx.listener(
                                                                        move |this, _, _, cx| {
                                                                            cx.stop_propagation();
                                                                            this.remove_track_from_current_playlist(
                                                                                track_id,
                                                                            );
                                                                            cx.notify();
                                                                        },
                                                                    ))
                                                                    .child(lucide_icon(
                                                                        "minus",
                                                                        14.0,
                                                                        rgb(0xFFB4B4),
                                                                    )),
                                                            )
                                                        }),
                                                )
                                        },
                                    ))
                                    .when(self.next_page_url.is_some(), |list| {
                                        list.child(
                                            div()
                                                .id("load-more-search-btn")
                                                .mt(px(4.0))
                                                .py(px(9.0))
                                                .rounded(px(8.0))
                                                .bg(rgb(pal.bg_chip))
                                                .border_1()
                                                .border_color(rgb(pal.border_strong))
                                                .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_size(px(11.5))
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(rgb(pal.accent_soft_text))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.load_more_search();
                                                    cx.notify();
                                                }))
                                                .child(if self.loading_more {
                                                    text(self.language, "Yükleniyor...")
                                                } else {
                                                    text(self.language, "Daha Fazla Yükle")
                                                }),
                                        )
                                    }),
                            ),
                    )
                    // SAĞ PANEL: ALBÜM KAPAĞI VEYA AKTİF ÇALMA SIRASI (QUEUE)
                    .child(
                        div()
                            .w(px(285.0))
                            .p(px(18.0))
                            .bg(Self::rgba_with_alpha(pal.bg_sidebar, 0xE6))
                            .border_l_1()
                            .border_color(rgb(pal.border_subtle))
                            .flex()
                            .flex_col()
                            .when(self.queue_drawer_open, |panel| {
                                panel
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .text_color(rgb(pal.accent_soft_text))
                                                    .child(format!(
                                                        "{} ({})",
                                                        text(self.language, "Çalma Sırası"),
                                                        self.queue.len()
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(6.0))
                                                    .when(!self.queue.is_empty(), |bar| {
                                                        bar.child(
                                                            div()
                                                                .id("clear-queue-btn")
                                                                .px(px(7.0))
                                                                .py(px(2.5))
                                                                .rounded(px(5.0))
                                                                .bg(rgb(pal.bg_chip))
                                                                .hover(|s| s.bg(rgb(0x7F1D1D)))
                                                                .cursor_pointer()
                                                                .flex()
                                                                .items_center()
                                                                .gap(px(4.0))
                                                                .text_size(px(11.0))
                                                                .text_color(rgb(pal.text_secondary))
                                                                .on_click(cx.listener(|this, _, _, cx| {
                                                                    this.clear_queue();
                                                                    cx.notify();
                                                                }))
                                                                .child(lucide_icon(
                                                                    "trash-2",
                                                                    11.5,
                                                                    rgb(pal.text_secondary),
                                                                ))
                                                                .child(text(self.language, "Temizle")),
                                                        )
                                                    })
                                                    .child(
                                                        div()
                                                            .id("close-queue-panel")
                                                            .p(px(4.0))
                                                            .rounded(px(5.0))
                                                            .bg(rgb(pal.bg_chip))
                                                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                                            .cursor_pointer()
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.search_focused = false;
                                                                this.queue_drawer_open = false;
                                                                cx.notify();
                                                            }))
                                                            .child(lucide_icon(
                                                                "x",
                                                                12.5,
                                                                rgb(pal.text_primary),
                                                            )),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("queue-list-scroll")
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .overflow_y_scroll()
                                            .children(self.queue.iter().enumerate().map(
                                                |(q_idx, q_track)| {
                                                    let is_q_cur = self.queue_index == Some(q_idx);
                                                    let q_thumb = self
                                                        .thumb_cache
                                                        .get(&q_track.album.cover_medium)
                                                        .cloned();
                                                    div()
                                                        .id(ElementId::Name(
                                                            format!(
                                                                "q-item-{q_idx}-{}",
                                                                q_track.id
                                                            )
                                                            .into(),
                                                        ))
                                                        .px(px(8.0))
                                                        .py(px(6.0))
                                                        .rounded(px(6.0))
                                                        .bg(if is_q_cur {
                                                            rgb(pal.bg_row_active)
                                                        } else {
                                                            rgb(pal.bg_row)
                                                        })
                                                        .hover(|s| s.bg(rgb(pal.bg_row_hover)))
                                                        .cursor_pointer()
                                                        .flex()
                                                        .items_center()
                                                        .justify_between()
                                                        .gap(px(8.0))
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.play_queue_index(q_idx);
                                                                cx.notify();
                                                            },
                                                        ))
                                                        .when_some(q_thumb, |row, path| {
                                                            row.child(
                                                                div()
                                                                    .w(px(28.0))
                                                                    .h(px(28.0))
                                                                    .rounded(px(4.0))
                                                                    .overflow_hidden()
                                                                    .child(img(path).size_full()),
                                                            )
                                                        })
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .flex_col()
                                                                .flex_1()
                                                                .overflow_hidden()
                                                                .child(
                                                                    div()
                                                                        .text_size(px(11.5))
                                                                        .font_weight(
                                                                            gpui::FontWeight::SEMIBOLD,
                                                                        )
                                                                        .text_color(if is_q_cur {
                                                                            rgb(pal.accent_soft_text)
                                                                        } else {
                                                                            rgb(pal.text_primary)
                                                                        })
                                                                        .child(format!(
                                                                            "{}. {}",
                                                                            q_idx + 1,
                                                                            q_track.title
                                                                        )),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_size(px(10.5))
                                                                        .text_color(rgb(
                                                                            pal.text_secondary,
                                                                        ))
                                                                        .child(
                                                                            q_track.artist.name.clone(),
                                                                        ),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .id(ElementId::Name(
                                                                    format!(
                                                                        "q-remove-{q_idx}-{}",
                                                                        q_track.id
                                                                    )
                                                                    .into(),
                                                                ))
                                                                .p(px(4.0))
                                                                .rounded(px(4.0))
                                                                .hover(|s| s.bg(rgb(0x7F1D1D)))
                                                                .cursor_pointer()
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                                    cx.stop_propagation();
                                                                    this.remove_queue_item(q_idx);
                                                                    cx.notify();
                                                                }))
                                                                .child(lucide_icon(
                                                                    "x",
                                                                    12.0,
                                                                    rgb(pal.text_secondary),
                                                                )),
                                                        )
                                                },
                                            )),
                                    )
                            })
                            .when(!self.queue_drawer_open, |panel| {
                                panel
                                    .items_center()
                                    .justify_center()
                                    .gap(px(16.0))
                                    .child(
                                        div()
                                            .w(px(216.0))
                                            .h(px(216.0))
                                            .rounded(px(16.0))
                                            .bg(rgb(pal.bg_panel))
                                            .border_2()
                                            .border_color(Self::rgba_with_alpha(ambient_hex, 0x88))
                                            .overflow_hidden()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when_some(
                                                self.cached_artwork_path.clone(),
                                                |box_div, path| {
                                                    box_div.child(img(path).size_full())
                                                },
                                            )
                                            .when(self.cached_artwork_path.is_none(), |box_div| {
                                                box_div.child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .items_center()
                                                        .gap(px(8.0))
                                                        .child(lucide_icon(
                                                            "disc-3",
                                                            42.0,
                                                            rgb(pal.accent),
                                                        ))
                                                        .child(
                                                            div()
                                                                .text_size(px(16.0))
                                                                .font_weight(
                                                                    gpui::FontWeight::BOLD,
                                                                )
                                                                .text_color(rgb(pal.accent))
                                                                .child("NOCTURNE"),
                                                        ),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .text_size(px(16.0))
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .child(title_text),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .text_color(rgb(pal.text_secondary))
                                                    .child(artist_text),
                                            ),
                                    )
                            }),
                    ),
            )
            // 3. ALT OYNATMA, KARIŞTIR/TEKRAR, SEEK VE SES BARI
            .child(
                div()
                    .h(px(88.0))
                    .px(px(24.0))
                    .bg(rgb(pal.bg_topbar))
                    .border_t_1()
                    .border_color(rgb(pal.border_subtle))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .w(px(40.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(pal.text_secondary))
                                    .child(Self::format_time(self.state.current_time)),
                            )
                            .child(
                                div()
                                    .id("seek-bar-track")
                                    .flex_1()
                                    .h(px(14.0))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, evt: &MouseDownEvent, _, cx| {
                                            if this.state.duration > 0.0 {
                                                let click_x: f32 = evt.position.x.into();
                                                let bar_left = 74.0;
                                                let ratio = ((click_x - bar_left) / seek_bar_width)
                                                    .clamp(0.0, 1.0)
                                                    as f64;
                                                let target_sec = ratio * this.state.duration;
                                                this.last_seek_at = Instant::now();

                                                if target_sec >= this.state.duration - 1.0 {
                                                    this.handle_track_ended();
                                                } else {
                                                    this.state.current_time = target_sec;
                                                    this.engine.seek_to(target_sec);
                                                }
                                                cx.notify();
                                            }
                                        }),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(5.0))
                                            .rounded(px(2.5))
                                            .bg(rgb(pal.bg_chip))
                                            .child(
                                                div()
                                                    .h_full()
                                                    .w(gpui::relative(progress_ratio))
                                                    .rounded(px(2.5))
                                                    .bg(rgb(pal.accent_bright)),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(40.0))
                                    .text_size(px(11.0))
                                    .text_color(rgb(pal.text_secondary))
                                    .child(Self::format_time(self.state.duration)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .w(px(220.0))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .id("shuffle-toggle-btn")
                                            .px(px(9.0))
                                            .py(px(4.5))
                                            .rounded(px(6.0))
                                            .bg(if self.shuffle_enabled {
                                                rgb(pal.accent)
                                            } else {
                                                rgb(pal.bg_chip)
                                            })
                                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap(px(5.0))
                                            .text_size(px(11.0))
                                            .text_color(if self.shuffle_enabled {
                                                rgb(pal.accent_soft_text)
                                            } else {
                                                rgb(pal.text_secondary)
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_shuffle();
                                                cx.notify();
                                            }))
                                            .child(lucide_icon(
                                                "shuffle",
                                                13.0,
                                                rgb(if self.shuffle_enabled {
                                                    pal.accent_soft_text
                                                } else {
                                                    pal.text_secondary
                                                }),
                                            ))
                                            .child(text(self.language, "Karıştır")),
                                    )
                                    .child(
                                        div()
                                            .id("repeat-toggle-btn")
                                            .px(px(9.0))
                                            .py(px(4.5))
                                            .rounded(px(6.0))
                                            .bg(if self.repeat_mode != RepeatMode::Off {
                                                rgb(pal.accent)
                                            } else {
                                                rgb(pal.bg_chip)
                                            })
                                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap(px(5.0))
                                            .text_size(px(11.0))
                                            .text_color(if self.repeat_mode != RepeatMode::Off {
                                                rgb(pal.accent_soft_text)
                                            } else {
                                                rgb(pal.text_secondary)
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.repeat_mode = this.repeat_mode.next();
                                                cx.notify();
                                            }))
                                            .child(lucide_icon(
                                                self.repeat_mode.icon_name(),
                                                13.0,
                                                rgb(if self.repeat_mode != RepeatMode::Off {
                                                    pal.accent_soft_text
                                                } else {
                                                    pal.text_secondary
                                                }),
                                            ))
                                            .child(self.repeat_mode.label(self.language)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(16.0))
                                    .child(
                                        div()
                                            .id("prev-btn")
                                            .w(px(34.0))
                                            .h(px(34.0))
                                            .rounded(px(17.0))
                                            .bg(rgb(pal.bg_chip))
                                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.play_prev();
                                                cx.notify();
                                            }))
                                            .child(lucide_icon(
                                                "skip-back",
                                                16.0,
                                                rgb(pal.text_primary),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("play-pause-btn")
                                            .w(px(42.0))
                                            .h(px(42.0))
                                            .rounded(px(21.0))
                                            .bg(rgb(pal.accent))
                                            .hover(|s| s.bg(rgb(pal.accent_bright)))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .on_click(cx.listener(|this, _, _, _| {
                                                this.engine.send_command("toggle");
                                            }))
                                            .child(lucide_icon(
                                                if self.state.is_playing {
                                                    "pause-filled"
                                                } else {
                                                    "play-filled"
                                                },
                                                18.0,
                                                rgb(pal.text_primary),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .id("next-btn")
                                            .w(px(34.0))
                                            .h(px(34.0))
                                            .rounded(px(17.0))
                                            .bg(rgb(pal.bg_chip))
                                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.play_next();
                                                cx.notify();
                                            }))
                                            .child(lucide_icon(
                                                "skip-forward",
                                                16.0,
                                                rgb(pal.text_primary),
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(220.0))
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .id("mute-toggle-btn")
                                            .p(px(4.0))
                                            .rounded(px(5.0))
                                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap(px(5.0))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_mute();
                                                cx.notify();
                                            }))
                                            .child(lucide_icon(
                                                self.volume_icon_name(),
                                                15.0,
                                                rgb(if self.muted {
                                                    pal.accent_bright
                                                } else {
                                                    pal.text_secondary
                                                }),
                                            ))
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(if self.muted {
                                                        rgb(pal.accent_bright)
                                                    } else {
                                                        rgb(pal.text_secondary)
                                                    })
                                                    .child(self.volume_label()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("volume-slider-track")
                                            .w(px(VOLUME_SLIDER_WIDTH))
                                            .h(px(16.0))
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(
                                                    move |this, evt: &MouseDownEvent, _, cx| {
                                                        let click_x: f32 = evt.position.x.into();
                                                        this.volume_dragging = true;
                                                        this.set_volume_from_mouse_x(
                                                            click_x,
                                                            volume_slider_right,
                                                        );
                                                        cx.notify();
                                                    },
                                                ),
                                            )
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(px(5.0))
                                                    .rounded(px(2.5))
                                                    .bg(rgb(pal.bg_chip))
                                                    .child(
                                                        div()
                                                            .h_full()
                                                            .w(gpui::relative(effective_volume))
                                                            .rounded(px(2.5))
                                                            .bg(rgb(pal.accent_bright)),
                                                    ),
                                            ),
                                    ),
                            ),
                    ),
            )
            // 4. AYRI MODÜLDEN GELEN AYARLAR PENCERESİ
            .when(self.settings_modal_open, |root| {
                root.child(self.render_settings_modal(pal, cx))
            })
            .when(self.new_playlist_open, |root| {
                root.child(self.render_new_playlist_modal(pal, cx))
            })
            .when(self.playlist_picker_track.is_some(), |root| {
                root.child(self.render_playlist_picker(pal, cx))
            })
            .when_some(self.action_notice.clone(), |root, (message, _)| {
                root.child(
                    div()
                        .absolute()
                        .bottom(px(104.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px(px(14.0))
                                .py(px(9.0))
                                .rounded(px(8.0))
                                .bg(rgb(pal.bg_panel))
                                .border_1()
                                .border_color(rgb(pal.border_strong))
                                .text_size(px(12.0))
                                .text_color(rgb(pal.text_primary))
                                .child(message),
                        ),
                )
            })
    }
}

#[cfg(windows)]
fn register_windows_app_identity() {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{
        RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
        REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    const AUMID: &str = "com.egehansimarikk.nocturne";

    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(AUMID));
    }

    thread::spawn(|| unsafe {
        let subkey = HSTRING::from(format!("Software\\Classes\\AppUserModelId\\{AUMID}"));
        let mut hkey = HKEY::default();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &subkey,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
        .is_ok()
        {
            let set_str = |name: PCWSTR, val: &str| {
                let wide: Vec<u16> = val.encode_utf16().chain(std::iter::once(0)).collect();
                let bytes =
                    std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
                let _ = RegSetValueExW(hkey, name, None, REG_SZ, Some(bytes));
            };

            set_str(w!("DisplayName"), "Nocturne");

            if let Ok(exe_path) = std::env::current_exe() {
                let icon_val = format!("{},0", exe_path.display());
                set_str(w!("IconUri"), &icon_val);
            }
        }
    });
}

fn main() {
    #[cfg(windows)]
    register_windows_app_identity();

    let app = gpui_platform::application().with_assets(NocturneAssets);

    app.run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
        let window_handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Nocturne".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(NocturneApp::new),
            )
            .expect("GPUI ana penceresi açılamadı");

        let view_entity = window_handle.entity(cx).ok();
        cx.on_window_closed(move |cx: &mut App, _window_id| {
            if let Some(ref view) = view_entity {
                let _ = view.update(cx, |app, _| {
                    app.discord_rpc.clear();
                    app.save_config();
                });
            }
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}