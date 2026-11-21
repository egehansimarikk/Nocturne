use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

const TRANSLATION_CHUNK_CHARS: usize = 1400;
const TRANSLATION_ERROR_MESSAGE: &str = "Çeviri yüklenemedi, tekrar deneyin.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricLine {
    pub time_sec: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackLyrics {
    pub track_key: String,
    pub synced: Vec<LyricLine>,
    pub plain: Option<String>,
    pub instrumental: bool,
}

impl TrackLyrics {
    pub fn is_empty(&self) -> bool {
        !self.instrumental
            && self.synced.is_empty()
            && self.plain.as_deref().unwrap_or("").trim().is_empty()
    }
}

#[derive(Debug, Clone)]
pub enum LyricsState {
    Idle,
    Loading { track_key: String },
    Loaded(TrackLyrics),
    NotFound { track_key: String },
    Error { track_key: String, message: String },
}

#[derive(Debug, Deserialize)]
struct LrclibRecord {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(default)]
    instrumental: bool,
}

struct LyricsInner {
    current_key: String,
    request_id: u64,
    not_found_cache: HashSet<String>,
    state: LyricsState,
}

#[derive(Debug, Clone, Default)]
pub struct LyricsTranslationSnapshot {
    pub lines: Option<Vec<String>>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct LyricsTranslationInner {
    cache: HashMap<String, Vec<String>>,
    current_key: Option<String>,
    lines: Option<Vec<String>>,
    loading: bool,
    error: Option<String>,
}

#[derive(Clone)]
pub struct LyricsManager {
    inner: Arc<Mutex<LyricsInner>>,
    translation: Arc<Mutex<LyricsTranslationInner>>,
    client: reqwest::blocking::Client,
    translation_client: Arc<reqwest::blocking::Client>,
}

impl Default for LyricsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsManager {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(4))
            .tcp_nodelay(true)
            .user_agent(format!(
                "Nocturne/{} (https://github.com/egehansimarikk/Nocturne)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("lyrics HTTP client could not be initialized");
        let translation_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(6))
            .tcp_nodelay(true)
            .user_agent(format!(
                "Nocturne/{} (https://github.com/egehansimarikk/Nocturne)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("translation HTTP client could not be initialized");

        Self {
            inner: Arc::new(Mutex::new(LyricsInner {
                current_key: String::new(),
                request_id: 0,
                not_found_cache: HashSet::new(),
                state: LyricsState::Idle,
            })),
            translation: Arc::new(Mutex::new(LyricsTranslationInner::default())),
            client,
            translation_client: Arc::new(translation_client),
        }
    }

    pub fn state(&self) -> LyricsState {
        self.inner.lock().unwrap().state.clone()
    }

    pub fn snapshot(&self) -> LyricsState {
        self.state()
    }

    pub fn translation_snapshot(&self) -> LyricsTranslationSnapshot {
        translation_snapshot(&self.translation.lock().unwrap())
    }

    pub fn clear_translation_error(&self) {
        self.translation.lock().unwrap().error = None;
    }

    pub fn translate_lines(
        &self,
        lines: &[String],
        target_language: &str,
    ) -> LyricsTranslationSnapshot {
        let cache_key = format!("{target_language}\0{}", lines.join("\n"));
        let mut translation = self.translation.lock().unwrap();

        if translation.current_key.as_deref() != Some(&cache_key) {
            translation.current_key = Some(cache_key.clone());
            translation.lines = None;
            translation.loading = false;
            translation.error = None;
        }

        if let Some(cached) = translation.cache.get(&cache_key) {
            translation.lines = Some(cached.clone());
            return translation_snapshot(&translation);
        }

        if translation.error.is_some() {
            return translation_snapshot(&translation);
        }

        if translation.loading || lines.is_empty() {
            if lines.is_empty() {
                translation.lines = Some(Vec::new());
            }
            return translation_snapshot(&translation);
        }

        let non_empty_lines: Vec<(usize, String)> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(index, line)| (index, line.clone()))
            .collect();
        if non_empty_lines.is_empty() {
            let empty_translation = vec![String::new(); lines.len()];
            translation
                .cache
                .insert(cache_key, empty_translation.clone());
            translation.lines = Some(empty_translation);
            return translation_snapshot(&translation);
        }

        translation.loading = true;
        let request_lines: Vec<String> = non_empty_lines
            .iter()
            .map(|(_, line)| line.clone())
            .collect();
        let line_count = lines.len();
        let target_language = target_language.to_string();
        let this = self.clone();
        thread::spawn(move || {
            let result = this
                .translate_batch(&request_lines, &target_language)
                .map(|translated| {
                    let mut aligned = vec![String::new(); line_count];
                    for ((index, _), translated_line) in non_empty_lines.into_iter().zip(translated)
                    {
                        aligned[index] = translated_line;
                    }
                    aligned
                });

            let mut translation = this.translation.lock().unwrap();
            if translation.current_key.as_deref() != Some(&cache_key) {
                return;
            }
            match result {
                Ok(lines) => {
                    translation.cache.insert(cache_key, lines.clone());
                    translation.lines = Some(lines);
                    translation.error = None;
                }
                Err(error) => {
                    translation.lines = None;
                    translation.error = Some(error);
                }
            }
            translation.loading = false;
        });

        translation_snapshot(&translation)
    }

    pub fn force_refresh(&self, title: &str, artist: &str, duration_sec: f64) {
        self.retry_current(title, artist, duration_sec);
    }

    /// Çalan parça değiştiğinde çağrılır; önce yerel önbelleğe bakar, yoksa LRCLIB'den çeker.
    pub fn fetch_for_track(&self, title: &str, artist: &str, duration_sec: f64) {
        self.fetch_for_track_inner(title, artist, duration_sec, false);
    }

    fn fetch_for_track_inner(&self, title: &str, artist: &str, duration_sec: f64, force: bool) {
        let clean_title = clean_query_text(title);
        let clean_artist = primary_artist(artist);
        if clean_title.is_empty() || clean_artist.is_empty() {
            return;
        }

        let key = make_track_key(&clean_artist, &clean_title);

        let request_id = {
            let mut guard = self.inner.lock().unwrap();
            if !force
                && guard.current_key == key
                && matches!(
                    guard.state,
                    LyricsState::Loading { .. } | LyricsState::Loaded(_)
                )
            {
                return;
            }

            if force {
                guard.not_found_cache.remove(&key);
                let _ = fs::remove_file(cache_file_path(&key));
            } else if guard.not_found_cache.contains(&key) {
                guard.request_id = guard.request_id.wrapping_add(1);
                guard.current_key = key.clone();
                guard.state = LyricsState::NotFound {
                    track_key: key.clone(),
                };
                return;
            }

            guard.current_key = key.clone();
            guard.request_id = guard.request_id.wrapping_add(1);
            let request_id = guard.request_id;
            guard.state = LyricsState::Loading {
                track_key: key.clone(),
            };
            request_id
        };

        if !force {
            if let Some(cached) = load_cached_lyrics(&key) {
                let mut guard = self.inner.lock().unwrap();
                if guard.current_key == key && guard.request_id == request_id {
                    guard.state = if cached.is_empty() {
                        guard.not_found_cache.insert(key.clone());
                        LyricsState::NotFound { track_key: key }
                    } else {
                        LyricsState::Loaded(cached)
                    };
                }
                return;
            }
        }

        let this = self.clone();
        let dur_rounded = if duration_sec.is_finite() && duration_sec > 0.0 {
            Some(duration_sec.round() as u64)
        } else {
            None
        };

        thread::spawn(move || {
            let result = this.query_lrclib(&key, &clean_artist, &clean_title, dur_rounded);
            let mut guard = this.inner.lock().unwrap();
            if guard.current_key != key || guard.request_id != request_id {
                return;
            }

            match result {
                Ok(Some(lyrics)) => {
                    guard.state = if lyrics.is_empty() {
                        guard.not_found_cache.insert(key.clone());
                        LyricsState::NotFound { track_key: key }
                    } else {
                        save_cached_lyrics(&lyrics);
                        LyricsState::Loaded(lyrics)
                    };
                }
                Ok(None) | Err(_) => {
                    guard.not_found_cache.insert(key.clone());
                    guard.state = LyricsState::NotFound { track_key: key };
                }
            }
        });
    }

    pub fn retry_current(&self, title: &str, artist: &str, duration_sec: f64) {
        self.fetch_for_track_inner(title, artist, duration_sec, true);
    }

    fn query_lrclib(
        &self,
        key: &str,
        artist: &str,
        title: &str,
        duration: Option<u64>,
    ) -> Result<Option<TrackLyrics>, String> {
        let mut get_url = format!(
            "https://lrclib.net/api/get?artist_name={}&track_name={}",
            url_encode(artist),
            url_encode(title)
        );
        if let Some(d) = duration {
            get_url.push_str(&format!("&duration={d}"));
        }

        let response = self
            .client
            .get(&get_url)
            .send()
            .map_err(|e| format!("Bağlantı hatası: {e}"))?;

        if response.status().is_success() {
            let record: LrclibRecord = response
                .json()
                .map_err(|e| format!("Söz verisi okunamadı: {e}"))?;
            let lyrics = record_to_lyrics(key, record);
            if !lyrics.is_empty() {
                return Ok(Some(lyrics));
            }
        } else if response.status() != reqwest::StatusCode::NOT_FOUND {
            return Err(format!("LRCLIB yanıt kodu: {}", response.status().as_u16()));
        }

        // Exact lookup may miss; use the single remaining request for search.
        let search_url = format!(
            "https://lrclib.net/api/search?artist_name={}&track_name={}",
            url_encode(artist),
            url_encode(title)
        );

        let resp = self
            .client
            .get(&search_url)
            .send()
            .map_err(|e| format!("Bağlantı hatası: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("LRCLIB yanıt kodu: {}", resp.status().as_u16()));
        }

        let records: Vec<LrclibRecord> = resp
            .json()
            .map_err(|e| format!("Söz verisi okunamadı: {e}"))?;

        let mut fallback = None;
        for record in records {
            let lyrics = record_to_lyrics(key, record);
            if lyrics.is_empty() {
                continue;
            }
            if !lyrics.synced.is_empty() {
                return Ok(Some(lyrics));
            }
            if fallback.is_none() {
                fallback = Some(lyrics);
            }
        }

        Ok(fallback)
    }

    fn translate_batch(
        &self,
        lines: &[String],
        target_language: &str,
    ) -> Result<Vec<String>, String> {
        let chunks = chunk_translation_lines(lines);
        let client = Arc::clone(&self.translation_client);
        let cancelled = Arc::new(AtomicBool::new(false));

        std::thread::scope(|scope| {
            let handles: Vec<_> = chunks
                .into_iter()
                .enumerate()
                .map(|(index, chunk)| {
                    let client = Arc::clone(&client);
                    let cancelled = Arc::clone(&cancelled);

                    scope.spawn(move || {
                        if cancelled.load(Ordering::Acquire) {
                            return Err(TRANSLATION_ERROR_MESSAGE.to_string());
                        }

                        let query = chunk.join("\n");
                        let url = format!(
                            "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={}&dt=t",
                            url_encode(target_language)
                        );
                        let result = match client
                            .post(url)
                            .form(&[("q", query)])
                            .send()
                            .map_err(|_| TRANSLATION_ERROR_MESSAGE.to_string())
                        {
                            Err(error) => Err(error),
                            Ok(response) if !response.status().is_success() => {
                                Err(TRANSLATION_ERROR_MESSAGE.to_string())
                            }
                            Ok(response) => {
                                let parsed: Result<Value, String> = response
                                    .json()
                                    .map_err(|error| {
                                        format!("Çeviri yanıtı okunamadı: {error}")
                                    });
                                match parsed {
                                    Err(error) => Err(error),
                                    Ok(parsed) => {
                                        parse_translation_response(&parsed, chunk.len())
                                            .map(|translated| (index, translated))
                                    }
                                }
                            }
                        };

                        if result.is_err() {
                            cancelled.store(true, Ordering::Release);
                        }
                        result
                    })
                })
                .collect();

            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                let result = handle
                    .join()
                    .map_err(|_| TRANSLATION_ERROR_MESSAGE.to_string());
                match result {
                    Ok(result) => results.push(result),
                    Err(error) => results.push(Err(error)),
                }
            }

            merge_parallel_translation_results(results)
        })
    }
}

fn merge_parallel_translation_results(
    mut results: Vec<Result<(usize, Vec<String>), String>>,
) -> Result<Vec<String>, String> {
    for result in &results {
        if let Err(error) = result {
            return Err(error.clone());
        }
    }

    results.sort_by_key(|result| match result {
        Ok((index, _)) => *index,
        Err(_) => usize::MAX,
    });

    let mut translated_lines = Vec::new();
    for result in results {
        if let Ok((_, chunk_lines)) = result {
            translated_lines.extend(chunk_lines);
        }
    }
    Ok(translated_lines)
}

fn chunk_translation_lines(lines: &[String]) -> Vec<Vec<String>> {
    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();
    let mut current_chars = 0;

    for line in lines {
        let line_chars = line.chars().count();
        let added_chars = line_chars + usize::from(!current_chunk.is_empty());
        if !current_chunk.is_empty() && current_chars + added_chars > TRANSLATION_CHUNK_CHARS {
            chunks.push(std::mem::take(&mut current_chunk));
            current_chars = 0;
        }

        if !current_chunk.is_empty() {
            current_chars += 1;
        }
        current_chars += line_chars;
        current_chunk.push(line.clone());
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }
    chunks
}

fn translation_snapshot(translation: &LyricsTranslationInner) -> LyricsTranslationSnapshot {
    LyricsTranslationSnapshot {
        lines: translation.lines.clone(),
        loading: translation.loading,
        error: translation.error.clone(),
    }
}

fn parse_translation_response(
    response: &Value,
    expected_lines: usize,
) -> Result<Vec<String>, String> {
    let segments = response
        .get(0)
        .and_then(Value::as_array)
        .ok_or_else(|| "Çeviri yanıtı beklenen biçimde değil.".to_string())?;
    let translated_segments: Vec<String> = segments
        .iter()
        .filter_map(|segment| segment.get(0).and_then(Value::as_str))
        .map(str::to_string)
        .collect();

    if translated_segments.len() == expected_lines {
        return Ok(translated_segments);
    }

    let joined = translated_segments.concat();
    let split_lines: Vec<String> = joined.lines().map(str::to_string).collect();
    if split_lines.len() == expected_lines {
        return Ok(split_lines);
    }

    let segment_lines: Vec<String> = translated_segments
        .join("\n")
        .lines()
        .map(str::to_string)
        .collect();
    if segment_lines.len() == expected_lines {
        return Ok(segment_lines);
    }

    Err(format!(
        "Çeviri satırları eşleştirilemedi ({} satır bekleniyordu).",
        expected_lines
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        chunk_translation_lines, merge_parallel_translation_results,
        parse_translation_response,
    };
    use serde_json::json;

    #[test]
    fn parses_translations_joined_with_newlines() {
        let response = json!([[["bonjour\nau revoir", "hello\ngoodbye"]], null, "fr"]);
        let translated = parse_translation_response(&response, 2).unwrap();
        assert_eq!(translated, ["bonjour", "au revoir"]);
    }

    #[test]
    fn chunks_translation_lines_without_reordering_them() {
        let lines = vec!["a".repeat(800), "b".repeat(800), "c".repeat(10)];
        let chunks = chunk_translation_lines(&lines);
        assert_eq!(
            chunks,
            vec![
                vec![lines[0].clone()],
                vec![lines[1].clone(), lines[2].clone()]
            ]
        );
    }

    #[test]
    fn merges_parallel_results_by_index() {
        let results = vec![
            Ok((2, vec!["uc".to_string()])),
            Ok((0, vec!["bir".to_string()])),
            Ok((1, vec!["iki".to_string()])),
        ];

        assert_eq!(
            merge_parallel_translation_results(results).unwrap(),
            vec!["bir", "iki", "uc"]
        );
    }
}

fn record_to_lyrics(key: &str, rec: LrclibRecord) -> TrackLyrics {
    let synced = rec
        .synced_lyrics
        .as_deref()
        .map(parse_lrc)
        .unwrap_or_default();

    let plain = rec
        .plain_lyrics
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());

    TrackLyrics {
        track_key: key.to_string(),
        synced,
        plain,
        instrumental: rec.instrumental,
    }
}

/// `[mm:ss.xx]` veya `[mm:ss.xxx]` zaman damgalı LRC metnini kronolojik satırlara dönüştürür.
pub fn parse_lrc(raw: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();

    for raw_line in raw.lines() {
        let mut rest = raw_line.trim();
        if rest.is_empty() {
            continue;
        }

        let mut timestamps = Vec::new();
        while rest.starts_with('[') {
            let Some(end_bracket) = rest.find(']') else {
                break;
            };
            let tag = &rest[1..end_bracket];
            if let Some(sec) = parse_lrc_timestamp(tag) {
                timestamps.push(sec);
                rest = rest[end_bracket + 1..].trim_start();
            } else {
                break;
            }
        }

        if timestamps.is_empty() {
            continue;
        }

        let lyric_text = rest.trim().to_string();
        for ts in timestamps {
            lines.push(LyricLine {
                time_sec: ts,
                text: lyric_text.clone(),
            });
        }
    }

    lines.sort_by(|a, b| {
        a.time_sec
            .partial_cmp(&b.time_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lines
}

fn parse_lrc_timestamp(tag: &str) -> Option<f64> {
    let (min_str, sec_str) = tag.split_once(':')?;
    let mins: f64 = min_str.trim().parse().ok()?;
    let secs: f64 = sec_str.trim().parse().ok()?;
    if !mins.is_finite() || !secs.is_finite() {
        return None;
    }
    Some(mins * 60.0 + secs)
}

/// Verilen oynatma saniyesine karşılık gelen aktif satır indeksini döner.
pub fn active_line_index(lines: &[LyricLine], position_sec: f64) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let pos = position_sec + 0.15; // Hafif görsel öngörü payı
    let mut active = None;
    for (idx, line) in lines.iter().enumerate() {
        if line.time_sec <= pos {
            active = Some(idx);
        } else {
            break;
        }
    }
    active
}

fn clean_query_text(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    for tag in [
        "(Explicit)",
        "[Explicit]",
        "(Clean)",
        "[Clean]",
        "(Remastered)",
        "[Remastered]",
        "(Radio Edit)",
        "[Radio Edit]",
        "- Remastered",
        "- Radio Edit",
    ] {
        if let Some(idx) = s.to_lowercase().find(&tag.to_lowercase()) {
            let cut = s[..idx].trim();
            if !cut.is_empty() {
                s = cut.to_string();
            }
        }
    }
    s.trim().to_string()
}

fn primary_artist(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((first, _)) = trimmed.split_once(", ") {
        if !first.trim().is_empty() {
            return first.trim().to_string();
        }
    }
    trimmed.to_string()
}

fn make_track_key(artist: &str, title: &str) -> String {
    let raw = format!(
        "{}_{}",
        artist.to_ascii_lowercase(),
        title.to_ascii_lowercase()
    );
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn lyrics_cache_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("Nocturne").join("lyrics_cache");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn cache_file_path(key: &str) -> PathBuf {
    let safe_name: String = key.chars().take(120).collect();
    lyrics_cache_dir().join(format!("{safe_name}.json"))
}

fn load_cached_lyrics(key: &str) -> Option<TrackLyrics> {
    let raw = fs::read_to_string(cache_file_path(key)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_cached_lyrics(lyrics: &TrackLyrics) {
    if let Ok(json) = serde_json::to_string(lyrics) {
        let _ = fs::write(cache_file_path(&lyrics.track_key), json);
    }
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
