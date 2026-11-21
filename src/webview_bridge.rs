use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, ICoreWebView2, ICoreWebView2Controller,
    ICoreWebView2Environment, ICoreWebView2EnvironmentOptions,
};
use webview2_com::{
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, WebMessageReceivedEventHandler,
};
use widestring::U16CString;
use windows::Foundation::{TypedEventHandler, Uri};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, PROPERTYKEY, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW,
    GetClientRect, HWND_BOTTOM, HWND_TOP, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW,
    QS_ALLINPUT, RegisterClassW, SW_SHOW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER, SetWindowPos, ShowWindow, TranslateMessage, WM_CLOSE, WM_DESTROY, WM_SIZE,
    WNDCLASSW, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPED, WS_SYSMENU,
    WS_THICKFRAME,
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR, factory};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AudioQuality {
    Standard,
    Superior,
    #[default]
    HiFi,
}

impl AudioQuality {
    pub const ALL: [Self; 3] = [Self::Standard, Self::Superior, Self::HiFi];

    pub fn as_code(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Superior => "superior",
            Self::HiFi => "hifi",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Standard => "Standart",
            Self::Superior => "Üstün",
            Self::HiFi => "Yüksek Doğruluk",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Standard => "128 kb/s",
            Self::Superior => "320 kb/s",
            Self::HiFi => "24-bit/192 kHz'e kadar",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackState {
    #[serde(default)]
    pub track_id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub is_playing: bool,
    #[serde(default)]
    pub current_time: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default = "default_volume")]
    pub volume: f64,
    #[serde(default)]
    pub pending_switch: bool,
}

fn default_volume() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPlaylist {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub nb_tracks: u64,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    State(PlaybackState),
    EqApplied(bool),
    QualityApplied(String),
    Spectrum([f32; 16]),
    Playlists(Vec<UserPlaylist>),
    UserId(u64),
    ActionResult {
        success: bool,
        message: String,
        kind: String,
        track_id: u64,
        playlist_id: u64,
    },
    TrackEnded,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EqState {
    enabled: bool,
    apply_in_webview: bool,
    bands: [f32; 10],
    preamp_db: f32,
}

impl Default for EqState {
    fn default() -> Self {
        Self {
            enabled: true,
            apply_in_webview: true,
            bands: [0.0; 10],
            preamp_db: 0.0,
        }
    }
}

impl EqState {
    fn init_json(self) -> String {
        serde_json::json!({
            "enabled": self.enabled,
            "apply_in_webview": self.apply_in_webview,
            "bands": self.bands,
            "preamp_db": self.preamp_db,
        })
        .to_string()
    }

    fn message(self) -> String {
        serde_json::json!({
            "type": "set_eq",
            "enabled": self.enabled,
            "bands": self.bands,
            "preamp_db": self.preamp_db,
            "apply_in_webview": self.apply_in_webview,
        })
        .to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamQualityState {
    quality: AudioQuality,
    normalize_volume: bool,
}

impl Default for StreamQualityState {
    fn default() -> Self {
        Self {
            quality: AudioQuality::HiFi,
            normalize_volume: false,
        }
    }
}

impl StreamQualityState {
    fn init_json(self) -> String {
        serde_json::json!({
            "quality": self.quality.as_code(),
            "normalize_volume": self.normalize_volume,
        })
        .to_string()
    }

    fn message(self) -> String {
        serde_json::json!({
            "type": "set_quality",
            "quality": self.quality.as_code(),
            "normalize_volume": self.normalize_volume,
        })
        .to_string()
    }
}

struct WebViewSharedInner {
    hwnd: Option<isize>,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    is_visible: bool,
    smtc: Option<SystemMediaTransportControls>,
    smtc_tx: Option<Sender<EngineEvent>>,
    last_smtc_track_id: u64,
    last_smtc_playing: Option<bool>,
    outbox_tx: Option<Sender<String>>,
    init_in_flight: bool,
    init_attempts: u32,
    eq: EqState,
    stream_quality: StreamQualityState,
    volume: f32,
    done: bool,
}

unsafe impl Send for WebViewSharedInner {}
unsafe impl Sync for WebViewSharedInner {}

struct WebViewShared(Arc<Mutex<WebViewSharedInner>>);

impl Clone for WebViewShared {
    fn clone(&self) -> Self {
        WebViewShared(Arc::clone(&self.0))
    }
}

impl WebViewShared {
    fn new(initial_eq: EqState, initial_volume: f32, initial_quality: StreamQualityState) -> Self {
        Self(Arc::new(Mutex::new(WebViewSharedInner {
            hwnd: None,
            controller: None,
            webview: None,
            is_visible: false,
            smtc: None,
            smtc_tx: None,
            last_smtc_track_id: 0,
            last_smtc_playing: None,
            outbox_tx: None,
            init_in_flight: false,
            init_attempts: 0,
            eq: initial_eq,
            stream_quality: initial_quality,
            volume: initial_volume.clamp(0.0, 1.0),
            done: false,
        })))
    }
}

static WEBVIEW_SHARED: std::sync::OnceLock<WebViewShared> = std::sync::OnceLock::new();
static WEBVIEW_THREAD_STOPPED: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();
static WEBVIEW_THREAD_INIT_FAIL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_webview_shared() -> Option<&'static WebViewShared> {
    WEBVIEW_SHARED.get()
}

pub struct DeezerEngine {
    pub rx: Receiver<EngineEvent>,
}

impl DeezerEngine {
    #[allow(dead_code)]
    pub fn pump(&self) {}
}

impl Drop for DeezerEngine {
    fn drop(&mut self) {
        if let Some(stopped) = WEBVIEW_THREAD_STOPPED.get() {
            stopped.store(true, Ordering::Relaxed);
        }
        if let Some(shared) = WEBVIEW_SHARED.get() {
            let mut sh = shared.0.lock().unwrap();
            sh.done = true;
        }
    }
}

impl DeezerEngine {
    /// WebView2'yi arka planda (görünmez pencerede) başlangıç EQ, ses seviyesi ve akış kalitesiyle başlatır.
    pub fn spawn_with_eq(
        enabled: bool,
        bands: [f32; 10],
        preamp_db: f32,
        apply_in_webview: bool,
        initial_volume: f32,
        quality: AudioQuality,
        normalize_volume: bool,
    ) -> Self {
        let initial_eq = EqState {
            enabled,
            apply_in_webview,
            bands,
            preamp_db,
        };
        let initial_quality = StreamQualityState {
            quality,
            normalize_volume,
        };
        let clamped_vol = initial_volume.clamp(0.0, 1.0);
        let (tx, rx) = mpsc::channel::<EngineEvent>();
        let (outbox_tx, outbox_rx) = mpsc::channel::<String>();
        let shared = WebViewShared::new(initial_eq, clamped_vol, initial_quality);

        {
            let mut sh = shared.0.lock().unwrap();
            sh.smtc_tx = Some(tx.clone());
            sh.outbox_tx = Some(outbox_tx);
            sh.eq = initial_eq;
            sh.stream_quality = initial_quality;
            sh.volume = clamped_vol;
            sh.init_attempts = 0;
        }

        let stopped = Arc::new(AtomicBool::new(false));
        let init_fail = Arc::new(AtomicBool::new(false));

        let shared_for_global = shared.clone();
        WEBVIEW_SHARED.get_or_init(|| shared_for_global.clone());
        WEBVIEW_THREAD_STOPPED.get_or_init(|| stopped.clone());
        WEBVIEW_THREAD_INIT_FAIL.get_or_init(|| init_fail.clone());

        let shared_for_thread = shared.clone();
        thread::spawn(move || {
            webview_thread_main(stopped, shared_for_thread, tx, outbox_rx);
        });

        Self { rx }
    }

    #[allow(dead_code)]
    pub fn spawn() -> Self {
        let def = EqState::default();
        Self::spawn_with_eq(
            def.enabled,
            def.bands,
            def.preamp_db,
            def.apply_in_webview,
            1.0,
            AudioQuality::HiFi,
            false,
        )
    }

    pub fn send_command(&self, cmd: &str) {
        let payload = serde_json::json!({ "type": "cmd", "val": cmd }).to_string();
        post_msg_to_webview(&payload);
    }

    pub fn play_track_id(&self, track_id: u64) {
        let payload = serde_json::json!({ "type": "play_track", "id": track_id }).to_string();
        post_msg_to_webview(&payload);
        self.refresh_playlists();
    }

    pub fn refresh_playlists(&self) {
        let payload = serde_json::json!({ "type": "fetch_playlists" }).to_string();
        post_msg_to_webview(&payload);
    }

    pub fn add_track_to_favorites(&self, track_id: u64) {
        let payload = serde_json::json!({ "type": "favorite_track", "id": track_id }).to_string();
        post_msg_to_webview(&payload);
    }

    pub fn remove_track_from_favorites(&self, track_id: u64) {
        let payload = serde_json::json!({ "type": "unfavorite_track", "id": track_id }).to_string();
        post_msg_to_webview(&payload);
    }

    pub fn add_track_to_playlist(&self, track_id: u64, playlist_id: u64) {
        let payload = serde_json::json!({
            "type": "add_track_to_playlist",
            "track_id": track_id,
            "playlist_id": playlist_id
        })
        .to_string();
        post_msg_to_webview(&payload);
    }

    pub fn remove_track_from_playlist(&self, track_id: u64, playlist_id: u64) {
        let payload = serde_json::json!({
            "type": "remove_track_from_playlist",
            "track_id": track_id,
            "playlist_id": playlist_id
        })
        .to_string();
        post_msg_to_webview(&payload);
    }

    pub fn create_playlist(&self, title: String, initial_track_id: Option<u64>) {
        let payload = serde_json::json!({
            "type": "create_playlist",
            "title": title,
            "track_id": initial_track_id
        })
        .to_string();
        post_msg_to_webview(&payload);
    }

    pub fn delete_playlist(&self, playlist_id: u64) {
        let payload = serde_json::json!({
            "type": "delete_playlist",
            "playlist_id": playlist_id
        })
        .to_string();
        post_msg_to_webview(&payload);
    }

    pub fn set_volume(&self, vol: f32) {
        let clamped = vol.clamp(0.0, 1.0);
        if let Some(sh) = WEBVIEW_SHARED.get() {
            let mut sh = sh.0.lock().unwrap();
            sh.volume = clamped;
        }
        let payload =
            serde_json::json!({ "type": "cmd", "val": format!("vol:{clamped:.3}") }).to_string();
        post_msg_to_webview(&payload);
    }

    pub fn seek_to(&self, seconds: f64) {
        let sec = seconds.max(0.0);
        let payload =
            serde_json::json!({ "type": "cmd", "val": format!("seek:{sec:.2}") }).to_string();
        post_msg_to_webview(&payload);
    }

    pub fn set_eq(&self, enabled: bool, bands: [f32; 10], preamp_db: f32, apply_in_webview: bool) {
        let state = EqState {
            enabled,
            apply_in_webview,
            bands,
            preamp_db,
        };

        if let Some(sh) = WEBVIEW_SHARED.get() {
            let mut sh = sh.0.lock().unwrap();
            sh.eq = state;
        }

        post_msg_to_webview(&state.message());
    }

    pub fn set_audio_quality(&self, quality: AudioQuality, normalize_volume: bool) {
        let state = StreamQualityState {
            quality,
            normalize_volume,
        };

        if let Some(sh) = WEBVIEW_SHARED.get() {
            let mut sh = sh.0.lock().unwrap();
            sh.stream_quality = state;
        }

        post_msg_to_webview(&state.message());
    }

    pub fn toggle_login_window(&self) -> bool {
        let (hwnd_raw, next_visible) = if let Some(sh) = WEBVIEW_SHARED.get() {
            let mut sh = sh.0.lock().unwrap();
            let hwnd_raw = sh.hwnd.unwrap_or(0);
            if hwnd_raw == 0 {
                (0, false)
            } else {
                sh.is_visible = !sh.is_visible;
                (hwnd_raw, sh.is_visible)
            }
        } else {
            (0, false)
        };

        if hwnd_raw == 0 {
            return false;
        }

        let hwnd = HWND(hwnd_raw as *mut _);
        unsafe {
            if next_visible {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    140,
                    90,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER,
                );
                let _ = ShowWindow(hwnd, SW_SHOW);
            } else {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_BOTTOM),
                    -32000,
                    -32000,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }

        if next_visible {
            post_eq_to_webview();
            post_volume_to_webview();
            post_quality_to_webview();
        }

        next_visible
    }
}

fn mark_init_failed() {
    if let Some(sh) = WEBVIEW_SHARED.get() {
        let mut sh = sh.0.lock().unwrap();
        sh.init_in_flight = false;
    }
}

fn post_eq_to_webview() {
    let Some(sh) = WEBVIEW_SHARED.get() else {
        return;
    };
    let state = sh.0.lock().unwrap().eq;
    let message = state.message();
    post_msg_to_webview(&message);
}

fn post_volume_to_webview() {
    let Some(sh) = WEBVIEW_SHARED.get() else {
        return;
    };
    let vol = sh.0.lock().unwrap().volume.clamp(0.0, 1.0);
    let payload = serde_json::json!({ "type": "cmd", "val": format!("vol:{vol:.3}") }).to_string();
    post_msg_to_webview(&payload);
}

fn post_quality_to_webview() {
    let Some(sh) = WEBVIEW_SHARED.get() else {
        return;
    };
    let state = sh.0.lock().unwrap().stream_quality;
    post_msg_to_webview(&state.message());
}

#[inline]
fn post_msg_to_webview(json_msg: &str) {
    let Some(sh) = WEBVIEW_SHARED.get() else {
        return;
    };
    let guard = sh.0.lock().unwrap();
    if let Some(ref tx) = guard.outbox_tx {
        let _ = tx.send(json_msg.to_string());
    }
}

fn apply_window_aumid(hwnd: HWND) {
    const PKEY_APPUSERMODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
        pid: 5,
    };

    unsafe {
        if let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd) {
            let prop = PROPVARIANT::from("com.egehansimarikk.nocturne");
            let _ = store.SetValue(&PKEY_APPUSERMODEL_ID, &prop);
            let _ = store.Commit();
        }
    }
}

fn bind_smtc_to_window(
    target_hwnd: HWND,
    tx: Sender<EngineEvent>,
) -> Option<SystemMediaTransportControls> {
    if target_hwnd.0.is_null() {
        return None;
    }

    apply_window_aumid(target_hwnd);

    let interop =
        factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>().ok()?;
    let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(target_hwnd).ok()? };

    let _ = smtc.SetIsEnabled(true);
    let _ = smtc.SetIsPlayEnabled(true);
    let _ = smtc.SetIsPauseEnabled(true);
    let _ = smtc.SetIsNextEnabled(true);
    let _ = smtc.SetIsPreviousEnabled(true);

    let handler = TypedEventHandler::<
        SystemMediaTransportControls,
        SystemMediaTransportControlsButtonPressedEventArgs,
    >::new(move |_, args| {
        if let Some(args) = args.as_ref()
            && let Ok(btn) = args.Button()
        {
            match btn {
                SystemMediaTransportControlsButton::Play
                | SystemMediaTransportControlsButton::Pause => {
                    let payload = serde_json::json!({ "type": "cmd", "val": "toggle" }).to_string();
                    post_msg_to_webview(&payload);
                }
                SystemMediaTransportControlsButton::Next => {
                    let _ = tx.send(EngineEvent::TrackEnded);
                }
                SystemMediaTransportControlsButton::Previous => {
                    let payload = serde_json::json!({ "type": "cmd", "val": "prev" }).to_string();
                    post_msg_to_webview(&payload);
                }
                _ => {}
            }
        }
        Ok(())
    });

    let _ = smtc.ButtonPressed(&handler);
    Some(smtc)
}

fn ensure_smtc_bound() -> Option<SystemMediaTransportControls> {
    if let Some(sh) = get_webview_shared() {
        let mut guard = sh.0.lock().unwrap();
        if guard.smtc.is_some() {
            return guard.smtc.clone();
        }

        let tx = guard.smtc_tx.clone()?;
        let main_title = U16CString::from_str("Nocturne").ok()?;
        let main_hwnd =
            unsafe { FindWindowW(PCWSTR::null(), PCWSTR(main_title.as_ptr())).unwrap_or_default() };

        let target_hwnd = if !main_hwnd.0.is_null() {
            main_hwnd
        } else {
            HWND(guard.hwnd.unwrap_or(0) as *mut _)
        };

        if target_hwnd.0.is_null() {
            return None;
        }

        let smtc = bind_smtc_to_window(target_hwnd, tx)?;
        guard.smtc = Some(smtc.clone());
        Some(smtc)
    } else {
        None
    }
}

fn update_native_smtc(st: &PlaybackState) {
    if st.pending_switch || st.title.is_empty() {
        return;
    }

    let Some(smtc) = ensure_smtc_bound() else {
        return;
    };

    if let Some(sh) = get_webview_shared() {
        let mut guard = sh.0.lock().unwrap();
        if guard.last_smtc_playing != Some(st.is_playing) {
            guard.last_smtc_playing = Some(st.is_playing);
            let status = if st.is_playing {
                MediaPlaybackStatus::Playing
            } else {
                MediaPlaybackStatus::Paused
            };
            let _ = smtc.SetPlaybackStatus(status);
        }

        if guard.last_smtc_track_id != st.track_id && st.track_id > 0 {
            guard.last_smtc_track_id = st.track_id;
            if let Ok(updater) = smtc.DisplayUpdater() {
                let _ = updater.SetType(MediaPlaybackType::Music);
                if let Ok(music) = updater.MusicProperties() {
                    let _ = music.SetTitle(&HSTRING::from(st.title.as_str()));
                    let _ = music.SetArtist(&HSTRING::from(st.artist.as_str()));
                }

                if !st.artwork.is_empty()
                    && let Ok(uri) = Uri::CreateUri(&HSTRING::from(st.artwork.as_str()))
                    && let Ok(stream_ref) = RandomAccessStreamReference::CreateFromUri(&uri)
                {
                    let _ = updater.SetThumbnail(&stream_ref);
                }

                let _ = updater.Update();
            }
        }
    }
}

fn resize_controller(hwnd: HWND, ctrl: &ICoreWebView2Controller) {
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(hwnd, &mut rect).is_ok() {
            let _ = ctrl.SetBounds(rect);
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SIZE => {
            if let Some(sh) = get_webview_shared() {
                let guard = sh.0.lock().unwrap();
                if let Some(ctrl) = &guard.controller {
                    resize_controller(hwnd, ctrl);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(sh) = get_webview_shared() {
                let mut guard = sh.0.lock().unwrap();
                guard.is_visible = false;
            }
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_BOTTOM),
                    -32000,
                    -32000,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn start_webview_environment(hwnd: HWND, tx: &Sender<EngineEvent>) {
    let tx = tx.clone();
    let (eq, vol, quality) = if let Some(sh) = WEBVIEW_SHARED.get() {
        let guard = sh.0.lock().unwrap();
        (guard.eq, guard.volume, guard.stream_quality)
    } else {
        (EqState::default(), 1.0, StreamQualityState::default())
    };

    let script = build_bridge_script(eq, vol, quality);

    unsafe {
        let data_dir = get_webview_data_dir();
        let data_dir_wide = U16CString::from_str(data_dir.to_string_lossy().as_ref()).unwrap();

        let env_opts: ICoreWebView2EnvironmentOptions =
            CoreWebView2EnvironmentOptions::default().into();
        let args_wide = U16CString::from_str(
            "--autoplay-policy=no-user-gesture-required --disable-features=HardwareMediaKeyHandling,MediaSessionService --disable-background-timer-throttling --disable-renderer-backgrounding",
        )
        .unwrap();
        let _ = env_opts.SetAdditionalBrowserArguments(PCWSTR(args_wide.as_ptr()));

        let env_handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |err, env: Option<ICoreWebView2Environment>| {
                let Some(env) = env else {
                    mark_init_failed();
                    return Ok(());
                };
                if err.is_err() {
                    mark_init_failed();
                    return Ok(());
                }

                let tx_clone = tx.clone();
                let script_for_controller = script.clone();
                let hwnd_clone = hwnd;

                let ctrl_handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                    move |err, ctrl: Option<ICoreWebView2Controller>| {
                        let Some(ctrl) = ctrl else {
                            mark_init_failed();
                            return Ok(());
                        };
                        if err.is_err() {
                            mark_init_failed();
                            return Ok(());
                        }

                        let _ = ctrl.SetIsVisible(true);
                        resize_controller(hwnd_clone, &ctrl);

                        let Ok(wv) = ctrl.CoreWebView2() else {
                            mark_init_failed();
                            return Ok(());
                        };

                        {
                            if let Ok(settings) = wv.Settings() {
                                let _ = settings.SetAreDefaultScriptDialogsEnabled(false);
                            }

                            let bridge_js = U16CString::from_str(&script_for_controller).unwrap();
                            let add_handler =
                                webview2_com::AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(
                                    Box::new(|_, _| Ok(())),
                                );
                            let _ = wv.AddScriptToExecuteOnDocumentCreated(
                                PCWSTR(bridge_js.as_ptr()),
                                &add_handler,
                            );

                            let tx_msg = tx_clone.clone();
                            let mut token = 0i64;
                            let msg_handler =
                                WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                                    if let Some(args) = args {
                                        let mut raw = PWSTR::null();
                                        if args.TryGetWebMessageAsString(&mut raw).is_ok()
                                            && !raw.is_null()
                                        {
                                            let msg = raw.to_string().unwrap_or_default();
                                            if let Some(json) = msg.strip_prefix("NOCTURNE_STATE:")
                                            {
                                                if let Ok(st) =
                                                    serde_json::from_str::<PlaybackState>(json)
                                                {
                                                    update_native_smtc(&st);
                                                    let _ = tx_msg.send(EngineEvent::State(st));
                                                }
                                            } else if let Some(json) =
                                                msg.strip_prefix("NOCTURNE_PLAYLISTS:")
                                            {
                                                if let Ok(list) =
                                                    serde_json::from_str::<Vec<UserPlaylist>>(json)
                                                {
                                                    let _ =
                                                        tx_msg.send(EngineEvent::Playlists(list));
                                                }
                                            } else if let Some(id_str) =
                                                msg.strip_prefix("NOCTURNE_USER_ID:")
                                                && let Ok(uid) = id_str.trim().parse::<u64>()
                                                && uid > 0
                                            {
                                                let _ = tx_msg.send(EngineEvent::UserId(uid));
                                            } else if let Some(json) =
                                                msg.strip_prefix("NOCTURNE_ACTION_RESULT:")
                                            {
                                                #[derive(Deserialize)]
                                                struct ActionResult {
                                                    success: bool,
                                                    message: String,
                                                    #[serde(default)]
                                                    kind: String,
                                                    #[serde(default)]
                                                    track_id: u64,
                                                    #[serde(default)]
                                                    playlist_id: u64,
                                                }
                                                if let Ok(result) =
                                                    serde_json::from_str::<ActionResult>(json)
                                                {
                                                    let _ =
                                                        tx_msg.send(EngineEvent::ActionResult {
                                                            success: result.success,
                                                            message: result.message,
                                                            kind: result.kind,
                                                            track_id: result.track_id,
                                                            playlist_id: result.playlist_id,
                                                        });
                                                }
                                            } else if msg == "NOCTURNE_TRACK_ENDED" {
                                                let _ = tx_msg.send(EngineEvent::TrackEnded);
                                            } else if msg == "NOCTURNE_READY" {
                                                let _ = tx_msg.send(EngineEvent::Ready);
                                                post_eq_to_webview();
                                                post_volume_to_webview();
                                                post_quality_to_webview();
                                                let payload =
                                                    serde_json::json!({ "type": "fetch_playlists" })
                                                        .to_string();
                                                post_msg_to_webview(&payload);
                                            } else if msg == "NOCTURNE_REQUEST_EQ" {
                                                post_eq_to_webview();
                                                post_volume_to_webview();
                                                post_quality_to_webview();
                                            } else if msg == "NOCTURNE_EQ_NOW" {
                                                post_eq_to_webview();
                                                post_volume_to_webview();
                                            } else if let Some(applied) =
                                                msg.strip_prefix("NOCTURNE_EQ_APPLIED:")
                                            {
                                                let _ = tx_msg.send(EngineEvent::EqApplied(
                                                    applied == "true",
                                                ));
                                            } else if let Some(q_code) =
                                                msg.strip_prefix("NOCTURNE_QUALITY_APPLIED:")
                                            {
                                                let _ = tx_msg.send(EngineEvent::QualityApplied(
                                                    q_code.to_string(),
                                                ));
                                            } else if let Some(raw_levels) =
                                                msg.strip_prefix("NOCTURNE_SPECTRUM:")
                                                && let Ok(levels) =
                                                    serde_json::from_str::<[f32; 16]>(raw_levels)
                                            {
                                                let _ = tx_msg.send(EngineEvent::Spectrum(levels));
                                            }
                                        }
                                    }
                                    Ok(())
                                }));
                            let _ = wv.add_WebMessageReceived(&msg_handler, &mut token);

                            let url = U16CString::from_str("https://www.deezer.com/").unwrap();
                            let _ = wv.Navigate(PCWSTR(url.as_ptr()));

                            let nav_tx = tx_clone.clone();
                            let mut nav_token = 0i64;
                            let nav_handler = webview2_com::NavigationCompletedEventHandler::create(
                                Box::new(move |_, _| {
                                    post_eq_to_webview();
                                    post_volume_to_webview();
                                    post_quality_to_webview();
                                    let payload = serde_json::json!({ "type": "fetch_playlists" })
                                        .to_string();
                                    post_msg_to_webview(&payload);
                                    let _ = nav_tx.send(EngineEvent::Ready);
                                    Ok(())
                                }),
                            );
                            let _ = wv.add_NavigationCompleted(&nav_handler, &mut nav_token);
                        }

                        if let Some(sh) = WEBVIEW_SHARED.get() {
                            let mut guard = sh.0.lock().unwrap();
                            guard.controller = Some(ctrl);
                            guard.webview = Some(wv);
                            guard.init_in_flight = false;
                            guard.init_attempts = 0;
                        }

                        Ok(())
                    },
                ));

                let _ = env.CreateCoreWebView2Controller(hwnd_clone, &ctrl_handler);
                Ok(())
            },
        ));

        let result = CreateCoreWebView2EnvironmentWithOptions(
            PCWSTR::null(),
            PCWSTR(data_dir_wide.as_ptr()),
            &env_opts,
            &env_handler,
        );
        if result.is_err() {
            mark_init_failed();
        }
    }
}

fn init_hidden_webview(tx: Sender<EngineEvent>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let _ =
            SetCurrentProcessExplicitAppUserModelID(&HSTRING::from("com.egehansimarikk.nocturne"));
    }

    let Some(hwnd) = ensure_host_window(&tx) else {
        mark_init_failed();
        return;
    };

    start_webview_environment(hwnd, &tx);
}

fn webview_thread_main(
    stopped: Arc<AtomicBool>,
    _shared: WebViewShared,
    tx: Sender<EngineEvent>,
    outbox_rx: std::sync::mpsc::Receiver<String>,
) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    init_hidden_webview(tx);

    let mut msg = MSG::default();
    loop {
        if stopped.load(Ordering::Relaxed) {
            break;
        }

        while let Ok(json_msg) = outbox_rx.try_recv() {
            if let Ok(wide) = U16CString::from_str(&json_msg) {
                unsafe {
                    if let Some(sh) = WEBVIEW_SHARED.get() {
                        let sh = sh.0.lock().unwrap();
                        if let Some(ref wv) = sh.webview {
                            let _ = wv.PostWebMessageAsString(PCWSTR(wide.as_ptr()));
                        }
                    }
                }
            }
        }

        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = MsgWaitForMultipleObjects(None, false, 4, QS_ALLINPUT);
        }
    }

    unsafe { CoUninitialize() };
}

fn ensure_host_window(tx: &Sender<EngineEvent>) -> Option<HWND> {
    unsafe {
        let existing = if let Some(sh) = get_webview_shared() {
            sh.0.lock().unwrap().hwnd.unwrap_or(0)
        } else {
            0
        };
        if existing != 0 {
            if let Some(sh) = get_webview_shared() {
                let mut sh = sh.0.lock().unwrap();
                sh.smtc_tx = Some(tx.clone());
            }
            return Some(HWND(existing as *mut _));
        }

        let hinstance = GetModuleHandleW(None).unwrap_or_default();
        let class_name = U16CString::from_str("NocturneWebViewHost").unwrap();
        let title = U16CString::from_str("Nocturne — Deezer Oturumu (arka plan)").unwrap();

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME,
            -32000,
            -32000,
            1100,
            740,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .unwrap_or_default();

        if hwnd.0.is_null() {
            return None;
        }

        apply_window_aumid(hwnd);

        let dark = BOOL::from(true);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        if let Some(sh) = get_webview_shared() {
            let mut sh = sh.0.lock().unwrap();
            sh.hwnd = Some(hwnd.0 as isize);
            sh.smtc_tx = Some(tx.clone());
        }

        Some(hwnd)
    }
}

fn get_webview_data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let nocturne_dir = base.join("Nocturne");
    let dir = nocturne_dir.join("WebView2");

    let legacy = nocturne_dir.join("WebView2Data_GPUI");
    if !dir.exists() && legacy.is_dir() && fs::rename(&legacy, &dir).is_err() {
        return legacy;
    }

    let _ = fs::create_dir_all(&dir);
    dir
}

fn build_bridge_script(eq: EqState, initial_volume: f32, quality: StreamQualityState) -> String {
    let vol_str = format!("{:.3}", initial_volume.clamp(0.0, 1.0));
    bridge_script_template()
        .replace("__NOCTURNE_EQ_INIT__", &eq.init_json())
        .replace("__NOCTURNE_VOL_INIT__", &vol_str)
        .replace("__NOCTURNE_QUALITY_INIT__", &quality.init_json())
}

fn bridge_script_template() -> &'static str {
    r#"
    (() => {
        const origAddEventListener = EventTarget.prototype.addEventListener;
        EventTarget.prototype.addEventListener = function(type, listener, options) {
            if (String(type).toLowerCase() === 'beforeunload') {
                return;
            }
            return origAddEventListener.call(this, type, listener, options);
        };

        Object.defineProperty(window, 'onbeforeunload', {
            configurable: true,
            get() { return null; },
            set(_) {}
        });

        window.__nocturneMediaElements = new Set();
        window.__nocturnePosState = { duration: 0, position: 0, updatedAt: 0 };
        const parsedInitVol = Number(__NOCTURNE_VOL_INIT__);
        window.__nocturneTargetVol = Number.isFinite(parsedInitVol)
            ? Math.max(0, Math.min(1, parsedInitVol))
            : 1.0;
        const nocturneQualityInit = __NOCTURNE_QUALITY_INIT__;
        window.__nocturneTargetQuality = String(nocturneQualityInit.quality || 'hifi');
        window.__nocturneNormalizeVolume = Boolean(nocturneQualityInit.normalize_volume);
        window.__nocturneLastQualityApplied = '';
        window.__nocturneActiveStreamFormat = '';
        window.__nocturneLastEndedAt = 0;
        window.__nocturneExpectedTrackId = 0;
        window.__nocturneSwitchTs = 0;
        window.__nocturneLastSeekAt = 0;
        window.__nocturneApiToken = '';

        function getDesiredFormatsForQuality(q) {
            if (q === 'standard') {
                return [
                    { cipher: 'BF_CBC_STRIPE', format: 'MP3_128' }
                ];
            }
            if (q === 'superior') {
                return [
                    { cipher: 'BF_CBC_STRIPE', format: 'MP3_320' },
                    { cipher: 'BF_CBC_STRIPE', format: 'MP3_128' }
                ];
            }
            return [
                { cipher: 'BF_CBC_STRIPE', format: 'FLAC' },
                { cipher: 'BF_CBC_STRIPE', format: 'MP3_320' },
                { cipher: 'BF_CBC_STRIPE', format: 'MP3_128' }
            ];
        }

        function rewriteGetUrlPayload(rawBody) {
            if (typeof rawBody !== 'string' || !rawBody.includes('formats')) {
                return rawBody;
            }
            try {
                const parsed = JSON.parse(rawBody);
                if (parsed && Array.isArray(parsed.media)) {
                    const desired = getDesiredFormatsForQuality(window.__nocturneTargetQuality);
                    for (const item of parsed.media) {
                        if (item && item.type === 'FULL' && Array.isArray(item.formats)) {
                            const cipher = item.formats[0]?.cipher || 'BF_CBC_STRIPE';
                            item.formats = desired.map(f => ({ cipher, format: f.format }));
                        }
                    }
                    return JSON.stringify(parsed);
                }
            } catch (_) {}
            return rawBody;
        }

        function inspectGetUrlResponse(jsonData) {
            try {
                const mediaEntry = jsonData?.data?.[0]?.media?.[0];
                const actualFormat = String(mediaEntry?.format || '');
                if (actualFormat) {
                    window.__nocturneActiveStreamFormat = actualFormat;
                    window.__nocturneLastQualityApplied = window.__nocturneTargetQuality;
                    window.chrome.webview.postMessage(
                        `NOCTURNE_QUALITY_APPLIED:${window.__nocturneTargetQuality}:true:${actualFormat}`
                    );
                }
            } catch (_) {}
        }

        // Deezer'ın media.deezer.com/v1/get_url isteklerini fetch seviyesinde yakala ve hedef kaliteye zorla
        const nativeFetch = window.fetch?.bind(window);
        if (nativeFetch) {
            window.fetch = async function(input, init) {
                const url = typeof input === 'string'
                    ? input
                    : (input && typeof input.url === 'string' ? input.url : '');

                if (url.includes('media.deezer.com') && url.includes('/get_url')) {
                    try {
                        let nextInit = init ? { ...init } : {};
                        if (typeof nextInit.body === 'string') {
                            nextInit.body = rewriteGetUrlPayload(nextInit.body);
                        } else if (input instanceof Request) {
                            const clonedText = await input.clone().text();
                            const rewritten = rewriteGetUrlPayload(clonedText);
                            input = new Request(input, { body: rewritten });
                            nextInit = undefined;
                        }
                        const response = await nativeFetch(input, nextInit);
                        response.clone().json().then(inspectGetUrlResponse).catch(() => {});
                        return response;
                    } catch (_) {}
                }

                return nativeFetch(input, init);
            };
        }

        // XMLHttpRequest fallback (eski/alternatif oynatıcı yolları için)
        const origXhrOpen = XMLHttpRequest.prototype.open;
        const origXhrSend = XMLHttpRequest.prototype.send;
        XMLHttpRequest.prototype.open = function(method, url, ...rest) {
            this.__nocturneUrl = String(url || '');
            return origXhrOpen.call(this, method, url, ...rest);
        };
        XMLHttpRequest.prototype.send = function(body) {
            if (this.__nocturneUrl && this.__nocturneUrl.includes('media.deezer.com') && this.__nocturneUrl.includes('/get_url')) {
                if (typeof body === 'string') {
                    body = rewriteGetUrlPayload(body);
                }
                this.addEventListener('load', () => {
                    try {
                        const parsed = JSON.parse(this.responseText);
                        inspectGetUrlResponse(parsed);
                    } catch (_) {}
                }, { once: true });
            }
            return origXhrSend.call(this, body);
        };

        function applyNocturneVolume(vol) {
            const clamped = Math.max(0, Math.min(1, Number(vol)));
            if (!Number.isFinite(clamped)) return;
            window.__nocturneTargetVol = clamped;
            if (window.playerProxy && window.playerProxy.control && typeof window.playerProxy.control.setVolume === 'function') {
                try { window.playerProxy.control.setVolume(clamped); } catch (_) {}
            } else if (window.dzPlayer && window.dzPlayer.control && typeof window.dzPlayer.control.setVolume === 'function') {
                try { window.dzPlayer.control.setVolume(clamped); } catch (_) {}
            }
            const list = [...document.querySelectorAll('audio, video'), ...window.__nocturneMediaElements];
            for (const el of list) {
                try {
                    if (Math.abs(el.volume - clamped) > 0.005) {
                        el.volume = clamped;
                    }
                    el.muted = clamped === 0;
                } catch (_) {}
            }
        }

        async function applyNocturneAudioQuality(q, normalize, forceRestartIfChanged = false) {
            const targetQ = String(q || window.__nocturneTargetQuality || 'hifi');
            const normBool = Boolean(normalize);
            const qualityChanged = window.__nocturneLastQualityApplied !== '' && window.__nocturneLastQualityApplied !== targetQ;
            window.__nocturneTargetQuality = targetQ;
            window.__nocturneNormalizeVolume = normBool;

            // 1. Varsa dahili kontrol nesnelerine de bilgiyi ilet
            try {
                const qualityNum = targetQ === 'standard' ? 1 : targetQ === 'superior' ? 3 : 9;
                const formatName = targetQ === 'standard' ? 'MP3_128' : targetQ === 'superior' ? 'MP3_320' : 'FLAC';
                for (const ctrl of [window.playerProxy?.control, window.dzPlayer?.control].filter(Boolean)) {
                    if (typeof ctrl.setFormat === 'function') ctrl.setFormat(formatName);
                    if (typeof ctrl.setQuality === 'function') ctrl.setQuality(qualityNum);
                }
            } catch (_) {}

            // 2. Ses normalizasyonu (ReplayGain) Web Audio preamp üzerinden de desteklenir, Deezer tarafını da güncel tut
            try {
                window.localStorage?.setItem('dz_normalize', normBool ? '1' : '0');
            } catch (_) {}

            // 3. Şarkı aktif olarak çalıyorsa ve kalite değiştiyse, get_url hook'unun yeni kalitede akış çekmesi için parçayı kaldığı yerden yenile
            if (forceRestartIfChanged && qualityChanged) {
                const song = getDzCurrentSong();
                const curId = Number(song?.SNG_ID || window.dzPlayer?.songId || 0);
                const media = getActiveMedia();
                const wasPlaying = media ? !media.paused : Boolean(window.dzPlayer?.playing);
                const resumeSec = media && isFinite(media.currentTime) ? media.currentTime : 0;
                if (curId > 0 && wasPlaying) {
                    await playViaDzPlayer(curId);
                    if (resumeSec > 1.5) {
                        setTimeout(() => {
                            window.__nocturneCommand(`seek:${resumeSec.toFixed(2)}`);
                        }, 550);
                    }
                }
            }

            try {
                window.chrome.webview.postMessage(
                    `NOCTURNE_QUALITY_APPLIED:${targetQ}:armed:${window.__nocturneActiveStreamFormat || 'pending'}`
                );
            } catch (_) {}
        }

        // Deezer web oynatıcısı localStorage veya kendi ses ikonu ile sesi değiştirirse Nocturne ayarına geri kilitle
        document.addEventListener('volumechange', (e) => {
            const target = e.target;
            if (target && (target.tagName === 'AUDIO' || target.tagName === 'VIDEO')) {
                const locked = window.__nocturneTargetVol;
                if (Math.abs(target.volume - locked) > 0.005) {
                    target.volume = locked;
                }
                if (target.muted !== (locked === 0)) {
                    target.muted = locked === 0;
                }
            }
        }, true);

        const nocturneEqInit = __NOCTURNE_EQ_INIT__;
        window.__nocturneEqEnabled = Boolean(nocturneEqInit.enabled);
        window.__nocturneEqApplyInWebView = Boolean(nocturneEqInit.apply_in_webview);
        window.__nocturneEqBands = Array.isArray(nocturneEqInit.bands) && nocturneEqInit.bands.length === 10
            ? nocturneEqInit.bands.map((gain) => {
                const value = Number(gain);
                return Number.isFinite(value) ? Math.max(-12, Math.min(12, value)) : 0;
            })
            : [0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        const nocturneEqPreampInit = Number(nocturneEqInit.preamp_db);
        window.__nocturneEqPreamp = Number.isFinite(nocturneEqPreampInit)
            ? Math.max(-12, Math.min(6, nocturneEqPreampInit))
            : 0;

        const eqMediaSources = new WeakSet();
        const mediaElementSources = new WeakMap();
        const attachingMedia = new WeakSet();
        const eqContexts = new Map();
        const eqNodes = new WeakSet();

        let eqRouteFound = false;
        let lastEqAppliedReport = null;

        const AudioContextCtor = window.AudioContext || window.webkitAudioContext;
        const nativeAudioConnect = window.AudioNode?.prototype?.connect;
        const nativeCreateMediaElementSource = AudioContextCtor?.prototype?.createMediaElementSource;

        const EQ_RETRY_BASE_MS = 120;
        const EQ_RETRY_STEP_MS = 80;
        const EQ_RETRY_MAX_ATTEMPTS = 80;

        function reportEqApplied() {
            const applied = window.__nocturneEqEnabled
                && window.__nocturneEqApplyInWebView
                && eqRouteFound;
            if (applied === lastEqAppliedReport) return;
            lastEqAppliedReport = applied;
            try {
                window.chrome.webview.postMessage('NOCTURNE_EQ_APPLIED:' + String(applied));
            } catch (_) {}
        }

        function resumeRunningEqContexts() {
            for (const chain of eqContexts.values()) {
                try {
                    if (chain.context.state === 'suspended') {
                        chain.context.resume().catch(() => {});
                    }
                } catch (_) {}
            }
        }

        function prepareCrossOriginForMedia(mediaElement) {
            if (!mediaElement) return;
            const sourceUrl = mediaElement.currentSrc || mediaElement.getAttribute('src') || mediaElement.src;
            if (!sourceUrl) return;
            try {
                const url = new URL(sourceUrl, location.href);
                if (
                    url.protocol !== 'blob:' &&
                    url.protocol !== 'data:' &&
                    url.origin !== location.origin &&
                    !mediaElement.crossOrigin
                ) {
                    mediaElement.crossOrigin = 'anonymous';
                }
            } catch (_) {}
        }

        function ensureEqRoute(mediaElement, attempt = 0) {
            if (!mediaElement || !window.__nocturneEqApplyInWebView) return;
            if (mediaElementSources.has(mediaElement)) {
                resumeRunningEqContexts();
                reportEqApplied();
                return;
            }
            attachFallbackMediaEq(mediaElement);
            if (mediaElementSources.has(mediaElement)) {
                resumeRunningEqContexts();
                reportEqApplied();
                return;
            }
            if (attempt >= EQ_RETRY_MAX_ATTEMPTS) return;
            const delay = attempt === 0 ? 40 : EQ_RETRY_BASE_MS + attempt * EQ_RETRY_STEP_MS;
            setTimeout(() => ensureEqRoute(mediaElement, attempt + 1), delay);
        }

        function ensureEqRouteOnActiveMedia() {
            if (!window.__nocturneEqApplyInWebView) return;
            const list = [...document.querySelectorAll('audio, video'), ...window.__nocturneMediaElements];
            for (const el of list) {
                bindMediaEvents(el);
                if (mediaElementSources.has(el)) {
                    resumeRunningEqContexts();
                    reportEqApplied();
                } else {
                    ensureEqRoute(el, 0);
                }
            }
        }

        function updateEqChain(chain) {
            const bands = window.__nocturneEqBands;
            const enabled = window.__nocturneEqEnabled && window.__nocturneEqApplyInWebView;
            const maxBoost = enabled ? Math.max(0, ...bands) : 0;
            const preampDb = enabled ? window.__nocturneEqPreamp : 0;
            chain.preamp.gain.setValueAtTime(
                Math.pow(10, (preampDb - maxBoost) / 20),
                chain.context.currentTime
            );
            for (let i = 0; i < chain.filters.length; i++) {
                chain.filters[i].gain.setValueAtTime(
                    enabled ? bands[i] : 0,
                    chain.context.currentTime
                );
            }
            chain.limiter.ratio.setValueAtTime(enabled ? 20 : 1, chain.context.currentTime);
        }

        function getEqChain(context) {
            let chain = eqContexts.get(context);
            if (chain) return chain;

            const preamp = context.createGain();
            const analyser = context.createAnalyser();
            analyser.fftSize = 2048;
            analyser.smoothingTimeConstant = 0.72;

            const limiter = context.createDynamicsCompressor();
            limiter.threshold.value = -1;
            limiter.knee.value = 0;
            limiter.ratio.value = 20;
            limiter.attack.value = 0.003;
            limiter.release.value = 0.08;

            const frequencies = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
            const filters = frequencies.map((frequency, index) => {
                const filter = context.createBiquadFilter();
                filter.type = index === 0 ? 'lowshelf' : index === 9 ? 'highshelf' : 'peaking';
                filter.frequency.value = Math.min(frequency, context.sampleRate * 0.45);
                filter.Q.value = 0.70710678;
                eqNodes.add(filter);
                return filter;
            });

            eqNodes.add(preamp);
            nativeAudioConnect.call(preamp, filters[0]);
            for (let i = 0; i < filters.length - 1; i++) {
                nativeAudioConnect.call(filters[i], filters[i + 1]);
            }
            nativeAudioConnect.call(filters[filters.length - 1], analyser);
            nativeAudioConnect.call(analyser, limiter);
            nativeAudioConnect.call(limiter, context.destination);

            eqNodes.add(limiter);
            eqNodes.add(analyser);

            chain = { context, preamp, filters, analyser, limiter };
            eqContexts.set(context, chain);
            updateEqChain(chain);
            return chain;
        }

        function canRouteMediaElement(mediaElement) {
            const sourceUrl = mediaElement.currentSrc || mediaElement.src;
            if (!sourceUrl) return false;
            prepareCrossOriginForMedia(mediaElement);
            try {
                const url = new URL(sourceUrl, location.href);
                return url.protocol === 'blob:' || url.protocol === 'data:'
                    || url.origin === location.origin || Boolean(mediaElement.crossOrigin);
            } catch (_) {
                return false;
            }
        }

        function attachFallbackMediaEq(mediaElement) {
            if (!window.__nocturneEqApplyInWebView || !nativeCreateMediaElementSource
                || !nativeAudioConnect || !mediaElement || mediaElementSources.has(mediaElement)
                || attachingMedia.has(mediaElement) || !canRouteMediaElement(mediaElement)) return;
            attachingMedia.add(mediaElement);
            let context;
            try {
                context = new AudioContextCtor();
                const source = nativeCreateMediaElementSource.call(context, mediaElement);
                const chain = getEqChain(context);
                nativeAudioConnect.call(source, chain.preamp);
                mediaElementSources.set(mediaElement, source);
                eqMediaSources.add(source);
                eqRouteFound = true;
                reportEqApplied();
                if (context.state === 'suspended') context.resume().catch(() => {});
            } catch (_) {
                if (context) context.close().catch(() => {});
            } finally {
                attachingMedia.delete(mediaElement);
            }
        }

        const spectrumEdges = [20, 40, 63, 100, 160, 250, 400, 630, 1000, 1600, 2500, 4000, 6300, 9000, 12000, 15000, 22000];
        let lastSpectrumLevels = new Array(16).fill(0.05);

        setInterval(() => {
            if (eqContexts.size === 0) return;
            const levels = new Array(16).fill(0.05);
            for (const chain of eqContexts.values()) {
                try {
                    const bins = new Float32Array(chain.analyser.frequencyBinCount);
                    chain.analyser.getFloatFrequencyData(bins);
                    const binHz = chain.context.sampleRate / chain.analyser.fftSize;
                    for (let i = 0; i < levels.length; i++) {
                        const first = Math.max(0, Math.floor(spectrumEdges[i] / binHz));
                        const last = Math.min(bins.length - 1, Math.ceil(spectrumEdges[i + 1] / binHz));
                        let power = 0;
                        let count = 0;
                        for (let bin = first; bin <= last; bin++) {
                            if (Number.isFinite(bins[bin])) {
                                power += Math.pow(10, bins[bin] / 10);
                                count++;
                            }
                        }
                        if (count > 0 && power > 0) {
                            const db = 10 * Math.log10(power / count);
                            levels[i] = Math.max(levels[i], Math.max(0.05, Math.min(1, (db + 75) / 65)));
                        }
                    }
                } catch (_) {}
            }
            for (let i = 0; i < levels.length; i++) {
                const smoothing = levels[i] > lastSpectrumLevels[i] ? 0.65 : 0.28;
                lastSpectrumLevels[i] = lastSpectrumLevels[i] * (1 - smoothing) + levels[i] * smoothing;
            }
            try {
                window.chrome.webview.postMessage('NOCTURNE_SPECTRUM:' + JSON.stringify(lastSpectrumLevels));
            } catch (_) {}
        }, 100);

        if (nativeCreateMediaElementSource && nativeAudioConnect) {
            AudioContextCtor.prototype.createMediaElementSource = function(mediaElement) {
                const existing = mediaElementSources.get(mediaElement);
                if (existing) return existing;
                prepareCrossOriginForMedia(mediaElement);
                const source = nativeCreateMediaElementSource.call(this, mediaElement);
                mediaElementSources.set(mediaElement, source);
                eqMediaSources.add(source);
                return source;
            };

            window.AudioNode.prototype.connect = function(destination, ...args) {
                if (
                    this.context &&
                    destination === this.context.destination &&
                    !eqNodes.has(this)
                ) {
                    try {
                        const chain = getEqChain(this.context);
                        const outputIdx = args.length > 0 ? args[0] : 0;
                        nativeAudioConnect.call(this, chain.preamp, outputIdx, 0);
                        eqRouteFound = true;
                        reportEqApplied();
                        return destination;
                    } catch (_) {
                        reportEqApplied();
                        return nativeAudioConnect.call(this, destination, ...args);
                    }
                }
                return nativeAudioConnect.call(this, destination, ...args);
            };
        }

        function stopBrowserPlaybackOnly() {
            try {
                if (window.playerProxy && typeof window.playerProxy.control?.pause === 'function') {
                    window.playerProxy.control.pause();
                } else if (window.dzPlayer && typeof window.dzPlayer.control?.pause === 'function') {
                    window.dzPlayer.control.pause();
                }
            } catch (_) {}
            try {
                const list = [...document.querySelectorAll('audio, video'), ...window.__nocturneMediaElements];
                for (const el of list) {
                    if (!el.paused) el.pause();
                }
            } catch (_) {}
        }

        function notifyTrackEndedOnce(mediaEl) {
            const now = Date.now();
            if (now - window.__nocturneLastEndedAt < 2500) return;
            window.__nocturneLastEndedAt = now;
            stopBrowserPlaybackOnly();
            try {
                if (mediaEl && !mediaEl.paused) {
                    mediaEl.pause();
                }
            } catch (_) {}
            try {
                window.chrome.webview.postMessage('NOCTURNE_TRACK_ENDED');
            } catch (_) {}
        }

        window.__nocturneApplyEq = (enabled, bands, applyInWebView, preampDb) => {
            window.__nocturneEqEnabled = Boolean(enabled);
            window.__nocturneEqApplyInWebView = Boolean(applyInWebView);
            const parsedPreamp = Number(preampDb);
            window.__nocturneEqPreamp = Number.isFinite(parsedPreamp)
                ? Math.max(-12, Math.min(6, parsedPreamp))
                : 0;
            if (Array.isArray(bands) && bands.length === 10) {
                window.__nocturneEqBands = bands.map((gain) => {
                    const value = Number(gain);
                    return Number.isFinite(value) ? Math.max(-12, Math.min(12, value)) : 0;
                });
            }
            for (const chain of eqContexts.values()) updateEqChain(chain);
            reportEqApplied();
            if (window.__nocturneEqApplyInWebView) {
                ensureEqRouteOnActiveMedia();
            }
        };

        function bindMediaEvents(el) {
            if (!el) return;
            window.__nocturneMediaElements.add(el);
            prepareCrossOriginForMedia(el);
            el.volume = window.__nocturneTargetVol;
            el.muted = window.__nocturneTargetVol === 0;
            if (el.__nocturneBound) return;
            el.__nocturneBound = true;

            el.addEventListener('ended', (e) => {
                e.stopImmediatePropagation();
                notifyTrackEndedOnce(el);
            }, true);

            el.addEventListener('timeupdate', () => {
                if (Date.now() - window.__nocturneLastSeekAt < 650) return;
                if (isFinite(el.duration) && el.duration > 3.0 && el.currentTime >= el.duration - 0.35) {
                    notifyTrackEndedOnce(el);
                }
            }, { passive: true });

            for (const evt of ['loadstart', 'loadeddata', 'canplay', 'play', 'playing', 'pause', 'loadedmetadata', 'durationchange', 'seeked']) {
                el.addEventListener(evt, () => {
                    applyNocturneVolume(window.__nocturneTargetVol);
                    if (window.__pushNocturneState) window.__pushNocturneState();
                    if (window.__nocturneEqApplyInWebView && evt !== 'pause') {
                        ensureEqRoute(el, 0);
                    }
                }, { passive: true });
            }
        }

        const origLoad = HTMLMediaElement.prototype.load;
        HTMLMediaElement.prototype.load = function(...args) {
            prepareCrossOriginForMedia(this);
            bindMediaEvents(this);
            applyNocturneVolume(window.__nocturneTargetVol);
            if (window.__nocturneEqApplyInWebView) ensureEqRoute(this, 0);
            return origLoad.apply(this, args);
        };

        const origPlay = HTMLMediaElement.prototype.play;
        HTMLMediaElement.prototype.play = function(...args) {
            prepareCrossOriginForMedia(this);
            bindMediaEvents(this);
            applyNocturneVolume(window.__nocturneTargetVol);
            if (window.__nocturneEqApplyInWebView) {
                ensureEqRoute(this, 0);
                setTimeout(() => ensureEqRoute(this, 0), 50);
                setTimeout(() => ensureEqRoute(this, 0), 200);
            }
            return origPlay.apply(this, args);
        };

        setInterval(() => {
            const list = [...document.querySelectorAll('audio, video'), ...window.__nocturneMediaElements];
            for (const el of list) {
                bindMediaEvents(el);
                if (window.__nocturneEqApplyInWebView && !el.paused && !mediaElementSources.has(el)) {
                    ensureEqRoute(el, 0);
                }
            }
        }, 250);

        const origSetActionHandler = navigator.mediaSession?.setActionHandler?.bind(navigator.mediaSession);
        window.__nocturneHandlers = {};
        if (origSetActionHandler) {
            navigator.mediaSession.setActionHandler = function(action, handler) {
                window.__nocturneHandlers[action] = handler;
                if (action === 'nexttrack') {
                    return origSetActionHandler(action, () => notifyTrackEndedOnce(getActiveMedia()));
                }
                return origSetActionHandler(action, handler);
            };
        }

        const origSetPositionState = navigator.mediaSession?.setPositionState?.bind(navigator.mediaSession);
        if (origSetPositionState) {
            navigator.mediaSession.setPositionState = function(state) {
                if (state && typeof state.duration === 'number' && isFinite(state.duration) && state.duration > 0) {
                    window.__nocturnePosState = {
                        duration: state.duration,
                        position: typeof state.position === 'number' && isFinite(state.position) ? state.position : 0,
                        updatedAt: Date.now()
                    };
                }
                const res = origSetPositionState(state);
                if (window.__pushNocturneState) window.__pushNocturneState();
                return res;
            };
        }

        function getActiveMedia() {
            const list = [...document.querySelectorAll('audio, video'), ...window.__nocturneMediaElements];
            list.forEach(bindMediaEvents);
            return list.find(m => !m.paused && m.readyState >= 2) || list.find(m => m.readyState >= 2) || list[0];
        }

        function getDzCurrentSong() {
            try {
                if (window.dzPlayer && typeof window.dzPlayer.getCurrentSong === 'function') {
                    return window.dzPlayer.getCurrentSong() || null;
                }
            } catch (_) {}
            return null;
        }

        async function getGwApiToken(forceRefresh = false) {
            if (!forceRefresh && window.__nocturneApiToken) return window.__nocturneApiToken;
            window.__nocturneApiToken = '';
            try {
                const r = await fetch(
                    'https://www.deezer.com/ajax/gw-light.php?method=deezer.getUserData&input=3&api_version=1.0&api_token=null',
                    { method: 'POST', credentials: 'include', body: '{}' }
                );
                const d = await r.json();
                const tok = d?.results?.checkForm || '';
                if (tok) window.__nocturneApiToken = tok;
                return tok;
            } catch (_) {
                return '';
            }
        }

        async function playViaDzPlayer(trackId) {
            const idStr = String(trackId);
            const dz = window.dzPlayer;
            if (!dz || typeof dz.play !== 'function') return false;
            try {
                const token = await getGwApiToken();
                if (token) {
                    const resp = await fetch(
                        `https://www.deezer.com/ajax/gw-light.php?method=song.getListData&input=3&api_version=1.0&api_token=${encodeURIComponent(token)}`,
                        {
                            method: 'POST',
                            credentials: 'include',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ sng_ids: [Number(trackId)] })
                        }
                    );
                    const json = await resp.json();
                    const songObj = json?.results?.data?.[0];
                    if (songObj && songObj.SNG_ID) {
                        const ok = await dz.play({
                            type: 'song',
                            id: idStr,
                            radio: false,
                            index: 0,
                            autoplay: true,
                            data: [songObj],
                            total: 1,
                            context: {
                                ID: idStr,
                                TYPE: 'track_page',
                                CONTEXT_ID: idStr
                            }
                        });
                        if (ok) return true;
                    }
                }
            } catch (_) {}
            try {
                const ok = await dz.play({
                    type: 'song',
                    id: idStr,
                    radio: false,
                    index: 0,
                    autoplay: true,
                    context: {
                        ID: idStr,
                        TYPE: 'track_page',
                        CONTEXT_ID: idStr
                    }
                });
                if (ok) return true;
            } catch (_) {}
            return false;
        }

        window.__nocturnePlayTrack = (trackId) => {
            const idNum = Number(trackId);
            window.__nocturneExpectedTrackId = idNum;
            window.__nocturneSwitchTs = Date.now();
            window.__nocturneLastEndedAt = 0;
            stopBrowserPlaybackOnly();
            applyNocturneVolume(window.__nocturneTargetVol);

            playViaDzPlayer(idNum).then((started) => {
                if (started || window.__nocturneExpectedTrackId !== idNum) return;
                let retries = 0;
                const retryTimer = setInterval(async () => {
                    retries += 1;
                    if (window.__nocturneExpectedTrackId !== idNum || retries > 15) {
                        clearInterval(retryTimer);
                        return;
                    }
                    if (await playViaDzPlayer(idNum)) {
                        applyNocturneVolume(window.__nocturneTargetVol);
                        clearInterval(retryTimer);
                    }
                }, 200);
            });
        };

        window.__nocturneFetchPlaylistsOnce = async () => {
            try {
                const r1 = await fetch(
                    'https://www.deezer.com/ajax/gw-light.php?method=deezer.getUserData&input=3&api_version=1.0&api_token=null',
                    { method: 'POST', credentials: 'include', body: '{}' }
                );
                const d1 = await r1.json();
                const res = d1?.results;
                const userId = Number(res?.USER?.USER_ID || 0);
                const apiToken = res?.checkForm || '';
                if (apiToken) window.__nocturneApiToken = apiToken;

                if (userId > 0) {
                    window.chrome.webview.postMessage('NOCTURNE_USER_ID:' + String(userId));
                    const embedded = res?.USER?.PLAYLISTS?.data || res?.PLAYLISTS?.data;
                    if (Array.isArray(embedded) && embedded.length > 0) {
                        const mapped = embedded.map(p => ({
                            id: Number(p.PLAYLIST_ID || p.id),
                            title: String(p.TITLE || p.title || 'İsimsiz Liste'),
                            nb_tracks: Number(p.NB_SONG || p.nb_tracks || 0)
                        })).filter(x => x.id > 0);
                        if (mapped.length > 0) {
                            postPlaylists(mapped);
                            return true;
                        }
                    }

                    const r2 = await fetch(`https://api.deezer.com/user/${userId}/playlists?limit=50`);
                    const d2 = await r2.json();
                    if (d2 && Array.isArray(d2.data) && d2.data.length > 0) {
                        const mapped = d2.data.map(p => ({
                            id: Number(p.id),
                            title: String(p.title || 'İsimsiz Liste'),
                            nb_tracks: Number(p.nb_tracks || 0)
                        }));
                        postPlaylists(mapped);
                        return true;
                    }

                    if (apiToken) {
                        const r3 = await fetch(
                            `https://www.deezer.com/ajax/gw-light.php?method=deezer.pageProfile&input=3&api_version=1.0&api_token=${encodeURIComponent(apiToken)}`,
                            {
                                method: 'POST',
                                credentials: 'include',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify({ USER_ID: userId, tab: 'playlists', nb: 50 })
                            }
                        );
                        const d3 = await r3.json();
                        const tabList = d3?.results?.TAB?.playlists?.data;
                        if (Array.isArray(tabList) && tabList.length > 0) {
                            const mapped = tabList.map(p => ({
                                id: Number(p.PLAYLIST_ID || p.id),
                                title: String(p.TITLE || p.title || 'İsimsiz Liste'),
                                nb_tracks: Number(p.NB_SONG || p.nb_tracks || 0)
                            })).filter(x => x.id > 0);
                            if (mapped.length > 0) {
                                postPlaylists(mapped);
                                return true;
                            }
                        }
                    }
                }
            } catch (_) {}
            return false;
        };

        let __nocturneSyncAttempt = 0;
        let __nocturneSyncTimer = null;
        let __nocturneLastSyncAt = 0;

        function postPlaylists(mapped) {
            try {
                window.chrome.webview.postMessage('NOCTURNE_PLAYLISTS:' + JSON.stringify(mapped));
            } catch (_) {}
        }

        window.__nocturneFetchPlaylists = async () => {
            const now = Date.now();
            if (now - __nocturneLastSyncAt < 250) return false;
            __nocturneLastSyncAt = now;
            if (__nocturneSyncTimer) {
                clearTimeout(__nocturneSyncTimer);
                __nocturneSyncTimer = null;
            }
            const ok = await window.__nocturneFetchPlaylistsOnce();
            if (ok) {
                __nocturneSyncAttempt = 0;
                window.__nocturneUserSynced = true;
                return true;
            }
            if (__nocturneSyncAttempt >= 60) return false;
            const delay = Math.min(4000, 300 * Math.pow(1.2, __nocturneSyncAttempt));
            __nocturneSyncAttempt += 1;
            __nocturneSyncTimer = setTimeout(() => {
                __nocturneSyncTimer = null;
                window.__nocturneFetchPlaylists();
            }, delay);
            return false;
        };

        window.__nocturneReportAction = (success, message, kind, trackId, playlistId = 0) => {
            if (window.chrome && window.chrome.webview) {
                window.chrome.webview.postMessage('NOCTURNE_ACTION_RESULT:' + JSON.stringify({
                    success,
                    message,
                    kind,
                    track_id: Number(trackId || 0),
                    playlist_id: Number(playlistId || 0)
                }));
            }
        };

        async function gwPost(method, input, allowTokenRetry = true) {
            const token = await getGwApiToken();
            if (!token) throw new Error('Önce Deezer hesabında oturum aç.');
            const response = await fetch(
                `https://www.deezer.com/ajax/gw-light.php?method=${method}&input=3&api_version=1.0&api_token=${encodeURIComponent(token)}`,
                {
                    method: 'POST',
                    credentials: 'include',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(input || {})
                }
            );
            if (!response.ok) throw new Error(`Deezer yanıt vermedi (${response.status}).`);
            const data = await response.json();
            const error = data?.error;
            const hasError = Boolean(error) &&
                (typeof error !== 'object' || Object.keys(error).length > 0);
            if (hasError || data?.results === false) {
                const details = typeof error === 'string'
                    ? error
                    : error?.MESSAGE || error?.message || error?.DATA_ERROR ||
                        Object.values(error || {}).filter(Boolean).join('; ');
                const errText = String(details || 'İşlem Deezer tarafından reddedildi.');
                if (
                    allowTokenRetry &&
                    (errText.includes('token') || errText.includes('CSRF') || errText.includes('VALID_TOKEN_REQUIRED'))
                ) {
                    window.__nocturneApiToken = '';
                    return gwPost(method, input, false);
                }
                throw new Error(errText);
            }
            return data?.results;
        }

        window.__nocturneAddLibraryItem = async (kind, trackId, playlistId) => {
            try {
                const id = Number(trackId);
                const plId = Number(playlistId || 0);
                if (!Number.isSafeInteger(id) || id <= 0) throw new Error('Geçersiz şarkı.');
                const isFavorite = kind === 'favorite';
                const method = isFavorite ? 'song.addFavorites' : 'playlist.addSongs';
                const input = isFavorite
                    ? { IDS: [String(id)] }
                    : {
                        playlist_id: String(plId),
                        songs: [[String(id), 0]],
                        offset: -1,
                        ctxt: { id: null, t: null }
                    };

                await gwPost(method, input);

                window.__nocturneReportAction(
                    true,
                    isFavorite
                        ? 'Şarkı beğenilenlere eklendi.'
                        : 'Şarkı çalma listesine eklendi.',
                    kind,
                    id,
                    plId
                );
                if (!isFavorite) setTimeout(window.__nocturneFetchPlaylists, 500);
            } catch (error) {
                window.__nocturneReportAction(
                    false,
                    error?.message || 'Deezer işlem hatası.',
                    kind,
                    trackId,
                    playlistId
                );
            }
        };

        window.__nocturneRemoveFavorite = async (trackId) => {
            const id = Number(trackId);
            try {
                if (!Number.isSafeInteger(id) || id <= 0) throw new Error('Geçersiz şarkı.');
                await gwPost('song.removeFavorites', { IDS: [String(id)] });
                window.__nocturneReportAction(
                    true,
                    'Şarkı beğenilenlerden çıkarıldı.',
                    'unfavorite',
                    id,
                    0
                );
            } catch (error) {
                window.__nocturneReportAction(
                    false,
                    error?.message || 'Deezer işlem hatası.',
                    'unfavorite',
                    id,
                    0
                );
            }
        };

        async function findTrackOffsetInPlaylist(plId, trackId) {
            try {
                const res = await gwPost('deezer.pagePlaylist', {
                    playlist_id: String(plId),
                    lang: 'tr',
                    nb: 2000,
                    start: 0,
                    tab: 0,
                    tags: true,
                    header: true
                });
                const songs = res?.SONGS?.data;
                if (Array.isArray(songs)) {
                    const idx = songs.findIndex(
                        s => Number(s?.SNG_ID || s?.id || 0) === Number(trackId)
                    );
                    if (idx >= 0) return idx;
                }
            } catch (_) {}
            return 0;
        }

        window.__nocturneRemoveTrackFromPlaylist = async (trackId, playlistId) => {
            const id = Number(trackId);
            const plId = Number(playlistId);
            try {
                if (!Number.isSafeInteger(id) || id <= 0) throw new Error('Geçersiz şarkı.');
                if (!Number.isSafeInteger(plId) || plId <= 0) {
                    throw new Error('Geçersiz çalma listesi.');
                }

                const offset = await findTrackOffsetInPlaylist(plId, id);

                const attempts = [
                    ['playlist.deleteSongs', {
                        playlist_id: String(plId),
                        songs: [[String(id), offset]],
                        ctxt: { id: null, t: null }
                    }],
                    ['playlist.deleteSongs', {
                        playlist_id: String(plId),
                        songs: [[String(id), 0]],
                        ctxt: { id: null, t: null }
                    }],
                    ['playlist.deleteSongs', {
                        playlist_id: String(plId),
                        songs: [String(id)],
                        ctxt: { id: null, t: null }
                    }]
                ];

                let removed = false;
                let lastErr = null;
                for (const [method, payload] of attempts) {
                    try {
                        await gwPost(method, payload);
                        removed = true;
                        break;
                    } catch (err) {
                        lastErr = err;
                    }
                }
                if (!removed) throw lastErr || new Error('Şarkı çalma listesinden çıkarılamadı.');

                window.__nocturneReportAction(
                    true,
                    'Şarkı çalma listesinden çıkarıldı.',
                    'playlist_track_removed',
                    id,
                    plId
                );
                setTimeout(window.__nocturneFetchPlaylists, 500);
            } catch (error) {
                window.__nocturneReportAction(
                    false,
                    error?.message || 'Deezer işlem hatası.',
                    'playlist_track_remove_error',
                    id,
                    plId
                );
            }
        };

        async function collectUserPlaylists() {
            try {
                const r1 = await fetch(
                    'https://www.deezer.com/ajax/gw-light.php?method=deezer.getUserData&input=3&api_version=1.0&api_token=null',
                    { method: 'POST', credentials: 'include', body: '{}' }
                );
                const d1 = await r1.json();
                const res = d1?.results;
                const token = res?.checkForm || '';
                if (token) window.__nocturneApiToken = token;

                const embedded = res?.USER?.PLAYLISTS?.data || res?.PLAYLISTS?.data;
                if (Array.isArray(embedded) && embedded.length > 0) {
                    return embedded.map(p => ({
                        id: Number(p.PLAYLIST_ID || p.id),
                        title: String(p.TITLE || p.title || '')
                    })).filter(x => x.id > 0);
                }

                const userId = Number(res?.USER?.USER_ID || 0);
                if (!(userId > 0)) return [];

                const r2 = await fetch(`https://api.deezer.com/user/${userId}/playlists?limit=100`);
                const d2 = await r2.json();
                if (d2 && Array.isArray(d2.data) && d2.data.length > 0) {
                    return d2.data.map(p => ({
                        id: Number(p.id),
                        title: String(p.title || '')
                    })).filter(x => x.id > 0);
                }

                const apiToken = window.__nocturneApiToken;
                if (apiToken) {
                    const r3 = await fetch(
                        `https://www.deezer.com/ajax/gw-light.php?method=deezer.pageProfile&input=3&api_version=1.0&api_token=${encodeURIComponent(apiToken)}`,
                        {
                            method: 'POST',
                            credentials: 'include',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({ USER_ID: userId, tab: 'playlists', nb: 100 })
                        }
                    );
                    const d3 = await r3.json();
                    const tabList = d3?.results?.TAB?.playlists?.data;
                    if (Array.isArray(tabList)) {
                        return tabList.map(p => ({
                            id: Number(p.PLAYLIST_ID || p.id),
                            title: String(p.TITLE || p.title || '')
                        })).filter(x => x.id > 0);
                    }
                }
            } catch (_) {}
            return [];
        }

        function matchesPlaylistTitle(playlist, name) {
            return String(playlist?.title || '').trim().toLowerCase() ===
                String(name || '').trim().toLowerCase();
        }

        const NOCTURNE_PLAYLIST_VERIFY_BUDGET_MS = 12000;

        async function waitForPlaylist(name, knownId, beforeIds) {
            const deadline = Date.now() + NOCTURNE_PLAYLIST_VERIFY_BUDGET_MS;
            let attempt = 0;
            while (Date.now() < deadline) {
                const list = await collectUserPlaylists();
                if (knownId > 0) {
                    const byId = list.find(p => p.id === knownId);
                    if (byId) return byId;
                }
                const byTitle = list.find(p => matchesPlaylistTitle(p, name));
                if (byTitle && (knownId > 0 || !beforeIds.has(byTitle.id))) return byTitle;
                attempt += 1;
                const remaining = deadline - Date.now();
                if (remaining <= 0) break;
                const pause = Math.min(1500, 300 + attempt * 120);
                await new Promise(resolve => setTimeout(resolve, Math.min(pause, remaining)));
            }
            return null;
        }

        window.__nocturneCreatePlaylist = async (title, trackId) => {
            const name = String(title || '').trim();
            const seedTrackId = Number(trackId || 0);
            try {
                if (!name) throw new Error('Çalma listesi adı boş olamaz.');
                const before = await collectUserPlaylists();
                const beforeIds = new Set(before.map(p => p.id));
                let createError = null;
                let createdId = 0;
                try {
                    const results = await gwPost('playlist.create', {
                        title: name,
                        description: ''
                    });
                    const nested = results?.DATA || results?.PLAYLIST || results || {};
                    createdId = Number(
                        results?.PLAYLIST_ID || results?.PLAYLIST?.PLAYLIST_ID ||
                        results?.playlist_id || nested?.PLAYLIST_ID ||
                        nested?.playlist_id || nested?.id || 0
                    );
                } catch (error) {
                    createError = error;
                }

                let playlist = createdId > 0 ? { id: createdId, title: name } : null;
                if (!playlist || createError) {
                    const verified = await waitForPlaylist(name, createdId, beforeIds);
                    if (verified) playlist = verified;
                }
                if (!playlist && createError) throw createError;
                if (!playlist) playlist = { id: 0, title: name };

                if (Number.isSafeInteger(seedTrackId) && seedTrackId > 0 && playlist.id > 0) {
                    try {
                        await gwPost('playlist.addSongs', {
                            playlist_id: String(playlist.id),
                            songs: [[String(seedTrackId), 0]],
                            offset: -1,
                            ctxt: { id: null, t: null }
                        });
                    } catch (_) {}
                }

                window.__nocturneReportAction(
                    true,
                    'Çalma listesi oluşturuldu.',
                    'playlist_created',
                    playlist.id,
                    playlist.id
                );
                window.__nocturneFetchPlaylists();
                setTimeout(window.__nocturneFetchPlaylists, 600);
                setTimeout(window.__nocturneFetchPlaylists, 2000);
            } catch (error) {
                window.__nocturneReportAction(
                    false,
                    error?.message || 'Deezer işlem hatası.',
                    'playlist_create_error',
                    0,
                    0
                );
            }
        };

        window.__nocturneDeletePlaylist = async (playlistId) => {
            const id = Number(playlistId);
            try {
                if (!Number.isSafeInteger(id) || id <= 0) {
                    throw new Error('Geçersiz çalma listesi.');
                }
                await gwPost('playlist.delete', { playlist_id: String(id) });
                window.__nocturneReportAction(
                    true,
                    'Çalma listesi silindi.',
                    'playlist_deleted',
                    id,
                    id
                );
                window.__nocturneFetchPlaylists();
                setTimeout(window.__nocturneFetchPlaylists, 700);
            } catch (error) {
                window.__nocturneReportAction(
                    false,
                    error?.message || 'Deezer işlem hatası.',
                    'playlist_delete_error',
                    id,
                    id
                );
            }
        };

        window.__nocturneCommand = (cmd) => {
            const media = getActiveMedia();
            const dz = window.dzPlayer;
            const proxy = window.playerProxy;
            if (cmd === 'toggle') {
                if (proxy && proxy.control && typeof proxy.control.togglePause === 'function') {
                    proxy.control.togglePause();
                } else if (dz && dz.control && typeof dz.control.togglePause === 'function') {
                    dz.control.togglePause();
                } else if (media) {
                    if (media.paused) {
                        if (window.__nocturneHandlers['play']) window.__nocturneHandlers['play']();
                        else media.play().catch(() => {});
                    } else {
                        if (window.__nocturneHandlers['pause']) window.__nocturneHandlers['pause']();
                        else media.pause();
                    }
                }
            } else if (cmd === 'restart') {
                window.__nocturneLastEndedAt = 0;
                let seekHandled = false;
                if (proxy && proxy.control && typeof proxy.control.seek === 'function') {
                    try { proxy.control.seek(0); seekHandled = true; } catch (_) {}
                } else if (dz && dz.control && typeof dz.control.seek === 'function') {
                    try { dz.control.seek(0); seekHandled = true; } catch (_) {}
                } else if (window.__nocturneHandlers['seekto']) {
                    try {
                        window.__nocturneHandlers['seekto']({ seekTime: 0 });
                        seekHandled = true;
                    } catch (_) {}
                }
                if (media) {
                    try { media.currentTime = 0; } catch (_) {}
                    media.play().catch(() => {});
                } else if (!seekHandled && dz && dz.control && typeof dz.control.seek === 'function') {
                    try { dz.control.seek(0); } catch (_) {}
                    if (typeof dz.control.togglePause === 'function' && !dz.playing) {
                        dz.control.togglePause();
                    }
                } else if (dz && !dz.playing && dz.control && typeof dz.control.togglePause === 'function') {
                    dz.control.togglePause();
                }
            } else if (cmd === 'stop') {
                window.__nocturneExpectedTrackId = 0;
                stopBrowserPlaybackOnly();
                if (media) {
                    try { media.currentTime = 0; } catch (_) {}
                }
            } else if (cmd === 'next') {
                notifyTrackEndedOnce(media);
            } else if (cmd === 'prev') {
                if (dz && dz.control && typeof dz.control.prevSong === 'function') {
                    dz.control.prevSong();
                } else if (window.__nocturneHandlers['previoustrack']) {
                    window.__nocturneHandlers['previoustrack']();
                }
            } else if (cmd.startsWith('seek:')) {
                const sec = parseFloat(cmd.slice(5));
                if (!isNaN(sec)) {
                    window.__nocturneLastSeekAt = Date.now();
                    const dur = Number(dz?.duration || dz?.getDuration?.() || media?.duration || 0);
                    const safeSec = dur > 0 ? Math.max(0, Math.min(dur - 0.6, sec)) : Math.max(0, sec);
                    if (proxy && proxy.control && typeof proxy.control.seek === 'function' && dur > 0) {
                        proxy.control.seek(Math.max(0, Math.min(0.995, safeSec / dur)));
                    } else if (dz && dz.control && typeof dz.control.seek === 'function' && dur > 0) {
                        dz.control.seek(Math.max(0, Math.min(0.995, safeSec / dur)));
                    } else if (window.__nocturneHandlers['seekto']) {
                        window.__nocturneHandlers['seekto']({ seekTime: safeSec });
                    } else if (media) {
                        media.currentTime = safeSec;
                    }
                }
            } else if (cmd.startsWith('vol:')) {
                const v = parseFloat(cmd.slice(4));
                if (!isNaN(v)) {
                    applyNocturneVolume(v);
                }
            } else if (cmd === 'eq_now') {
                ensureEqRouteOnActiveMedia();
            }
        };

        if (window.chrome && window.chrome.webview) {
            window.chrome.webview.addEventListener('message', (event) => {
                try {
                    const data = typeof event.data === 'string' ? JSON.parse(event.data) : event.data;
                    if (!data || !data.type) return;
                    if (data.type === 'play_track') {
                        window.__nocturnePlayTrack(data.id);
                    } else if (data.type === 'cmd') {
                        window.__nocturneCommand(data.val);
                    } else if (data.type === 'fetch_playlists') {
                        window.__nocturneFetchPlaylists();
                    } else if (data.type === 'favorite_track') {
                        window.__nocturneAddLibraryItem('favorite', data.id, 0);
                    } else if (data.type === 'unfavorite_track') {
                        window.__nocturneRemoveFavorite(data.id);
                    } else if (data.type === 'add_track_to_playlist') {
                        window.__nocturneAddLibraryItem('playlist', data.track_id, data.playlist_id);
                    } else if (data.type === 'remove_track_from_playlist') {
                        window.__nocturneRemoveTrackFromPlaylist(data.track_id, data.playlist_id);
                    } else if (data.type === 'create_playlist') {
                        window.__nocturneCreatePlaylist(data.title, data.track_id);
                    } else if (data.type === 'delete_playlist') {
                        window.__nocturneDeletePlaylist(data.playlist_id);
                    } else if (data.type === 'set_eq') {
                        window.__nocturneApplyEq(data.enabled, data.bands, data.apply_in_webview, data.preamp_db);
                    } else if (data.type === 'eq_now') {
                        window.__nocturneApplyEq(
                            data.enabled, data.bands, data.apply_in_webview, data.preamp_db
                        );
                        ensureEqRouteOnActiveMedia();
                    } else if (data.type === 'set_quality') {
                        applyNocturneAudioQuality(data.quality, data.normalize_volume, true);
                    }
                } catch (_) {}
            });
        }

        window.__pushNocturneState = () => {
            try {
                const song = getDzCurrentSong();
                const reportedId = Number(song?.SNG_ID || window.dzPlayer?.songId || 0);
                const expectedId = Number(window.__nocturneExpectedTrackId || 0);
                const switchAge = Date.now() - Number(window.__nocturneSwitchTs || 0);
                const isSwitching = expectedId > 0 && reportedId > 0 && reportedId !== expectedId && switchAge < 4000;
                if (expectedId > 0 && reportedId === expectedId) {
                    window.__nocturneSwitchTs = 0;
                }
                const meta = navigator.mediaSession?.metadata;
                const media = getActiveMedia();

                let art = '';
                if (song?.ALB_PICTURE) {
                    art = `https://e-cdns-images.dzcdn.net/images/cover/${song.ALB_PICTURE}/500x500-000000-80-0-0.jpg`;
                } else if (meta && meta.artwork && meta.artwork.length > 0) {
                    art = meta.artwork[meta.artwork.length - 1].src || meta.artwork[0].src || '';
                }

                const songTitle = song?.SNG_TITLE || meta?.title || '';
                const songArtist = song?.ART_NAME || meta?.artist || '';
                let cur = media && isFinite(media.currentTime)
                    ? media.currentTime
                    : Number(window.dzPlayer?.position || 0);
                let dur = media && isFinite(media.duration) && media.duration > 0
                    ? media.duration
                    : (Number(window.dzPlayer?.duration || song?.DURATION || 0) || window.__nocturnePosState.duration);
                const isPlaying = media
                    ? !media.paused
                    : Boolean(window.dzPlayer?.playing || navigator.mediaSession?.playbackState === 'playing');

                const seekSettled = Date.now() - window.__nocturneLastSeekAt > 650;
                if (!isSwitching && seekSettled && dur > 3.0 && cur >= dur - 0.35) {
                    notifyTrackEndedOnce(media);
                }

                const payload = {
                    TrackId: isSwitching ? expectedId : reportedId,
                    Title: isSwitching ? '' : songTitle,
                    Artist: isSwitching ? '' : songArtist,
                    Artwork: isSwitching ? '' : art,
                    IsPlaying: isSwitching ? true : isPlaying,
                    CurrentTime: isSwitching ? 0 : cur,
                    Duration: isSwitching ? 0 : dur,
                    Volume: media ? media.volume : window.__nocturneTargetVol,
                    PendingSwitch: isSwitching
                };
                window.chrome.webview.postMessage('NOCTURNE_STATE:' + JSON.stringify(payload));
            } catch (_) {}
        };

        const signalReady = () => {
            applyNocturneVolume(window.__nocturneTargetVol);
            if (!window.__nocturneReadySent) {
                window.__nocturneReadySent = true;
                try { window.chrome.webview.postMessage('NOCTURNE_READY'); } catch (_) {}
            }
            window.__nocturneFetchPlaylists();
            setTimeout(() => {
                applyNocturneAudioQuality(window.__nocturneTargetQuality, window.__nocturneNormalizeVolume, false);
            }, 1200);
        };

        if (location.hostname.endsWith('deezer.com')) {
            signalReady();
        }

        window.addEventListener('DOMContentLoaded', signalReady);
        window.addEventListener('load', signalReady);

        const requestEqState = () => {
            try { window.chrome.webview.postMessage('NOCTURNE_REQUEST_EQ'); } catch (_) {}
        };
        requestEqState();
        setTimeout(requestEqState, 800);

        setInterval(window.__pushNocturneState, 120);
        setInterval(window.__nocturneFetchPlaylists, 8000);
    })();
    "#
}
