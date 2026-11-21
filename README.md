# NOCTURNE

**Native, GPU-Accelerated Deezer Desktop Client for Windows built with Rust & GPUI.**

[🇬🇧 English](#-english) • [🇹🇷 Türkçe](#-türkçe)

---

## 🇬🇧 English

Nocturne is a high-performance, audiophile-grade desktop client engineered from the ground up in Rust. By combining Zed's hardware-accelerated **GPUI** framework with native **WebView2 COM** (Widevine & PlayReady DRM), **WASAPI** audio control, and a real-time **10-Band Biquad Equalizer**, Nocturne delivers ultra-low resource usage and studio-grade playback without Electron overhead.

> **⚠️ Proprietary Source-Available Software:** This project is **NOT** open-source. You are permitted to clone and build the source code locally on your own personal computer for private, non-commercial use only. Redistribution of source code or compiled binaries (`.exe`) is strictly prohibited. See [LICENSE](LICENSE) for full terms.

### ✨ Key Features

* **GPU-Accelerated Native UI (GPUI):** Smooth 60+ FPS rendering with dynamic album-art ambient lighting and custom themes (*Obsidian*, *Vinyl*, *Midnight*, etc.).
* **Audiophile Audio Engine (WASAPI & DSP):**
  * Supports **WASAPI Shared & Exclusive** output device selection with real-time sample rate / bit-depth diagnostics.
  * **10-Band Parametric/Graphic Equalizer** (`32 Hz` to `16 kHz`) with Preamp control, built-in audiophile curves (*Harman Over-Ear*, etc.), and custom user presets.
  * Hi-Fi FLAC / High Quality streaming selector and volume normalization.
* **DRM-Compliant Playback Bridge:** Direct native `webview2-com` integration supporting Widevine & PlayReady DRM without external browser bloat.
* **Social & Scrobbling Integrations:**
  * **Discord Rich Presence:** Real-time track title, artist, album artwork, and elapsed/remaining time sync.
  * **Last.fm Scrobbling:** Automatic *Now Playing* updates and scrobbling at 50% or 4-minute playback thresholds.
* **Full Library & Queue Management:**
  * Instant search with persistent search history, clipboard support (`Ctrl+V` / `Ctrl+C` / `Ctrl+X`), and paginated loading.
  * Access Liked Tracks, Artist Top Tracks, Album Views, and full User Playlist management (create, delete, add/remove tracks).
  * Smart playback queue with shuffle, repeat modes (`Off` / `List` / `One`), and browser-style Back/Forward navigation.
* **In-App Update Checker:** Automatic background update notifications and one-click download from GitHub Releases.
* **Multi-Language Support:** Automatic system language detection with Turkish and English localization.

### 🛠️ Personal Build Instructions (Windows)

#### Prerequisites
1. **Windows 10 / 11** (64-bit) with **Microsoft Edge WebView2 Runtime** installed (pre-installed on Windows 11).
2. **Rust Toolchain** (`stable-x86_64-pc-windows-msvc`) via [rustup.rs](https://rustup.rs/).
3. **Visual Studio Build Tools 2022** with the *"Desktop development with C++"* workload and Windows 10/11 SDK.

#### Building Locally
```bash
git clone https://github.com/egehansimarikk/nocturne.git
cd nocturne
cargo run --release
```

The compiled executable will be located at `target/release/nocturne.exe` for your personal use.

### ⌨️ Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Ctrl + F` | Focus search bar |
| `Ctrl + V` / `C` / `X` | Paste, copy, or cut in search / modal inputs |
| `Space` | Play / Pause (when search is unfocused) |
| `Ctrl + Left / Right` | Previous / Next track |
| `Alt + Left / Right` | Navigate view history Back / Forward |
| `M` | Mute / Unmute |
| `+` / `-` | Volume Up / Down |
| `Esc` | Close active modal or unfocus search |

### 📜 License & Copyright

**Copyright © 2026 Egehan Şımarık (`egehansimarikk`). All Rights Reserved.**

Nocturne is distributed under the **Nocturne Proprietary Source-Available License**:
* **Allowed:** Viewing the source code and compiling (`cargo build`) locally on your own personal computer for private, non-commercial use.
* **Prohibited:** Redistributing, uploading, hosting, or sharing the source code or compiled executables/installers, publishing modified forks, or any commercial use.

Read the complete [LICENSE](LICENSE) file before cloning or building.

---

## 🇹🇷 Türkçe

**Rust ve GPUI ile geliştirilmiş, Windows için Yerel ve GPU Hızlandırmalı Deezer Masaüstü İstemcisi.**

Nocturne, sıfırdan Rust ile mühendislik edilerek geliştirilmiş yüksek performanslı, odyofil seviyesinde bir masaüstü müzik istemcisidir. Zed'in donanım hızlandırmalı **GPUI** çatısını yerel **WebView2 COM** (Widevine & PlayReady DRM), **WASAPI** ses kontrolü ve gerçek zamanlı **10-Bant BiQuad Ekolayzer** ile birleştiren Nocturne; Electron yükü olmadan son derece düşük kaynak tüketimi ve stüdyo kalitesinde ses deneyimi sunar.

> **⚠️ Özel Mülkiyet Kaynak Kodu Açık Yazılım (Proprietary Source-Available):** Bu proje açık kaynak **DEĞİLDİR**. Kaynak kodu yalnızca kendi kişisel bilgisayarınızda özel, ticari olmayan kullanım amacıyla klonlayabilir ve derleyebilirsiniz. Kaynak kodun veya derlenmiş ikili dosyaların (`.exe`) yeniden dağıtılması kesinlikle yasaktır. Tüm koşullar için [LICENSE](LICENSE) dosyasına bakın.

### ✨ Öne Çıkan Özellikler

* **GPU Hızlandırmalı Yerel Arayüz (GPUI):** Dinamik albüm kapağı ambiyans aydınlatması ve özel temalar (*Obsidian*, *Vinyl*, *Midnight* vb.) ile akıcı 60+ FPS görüntüleme.
* **Odyofil Ses Motoru (WASAPI & DSP):**
  * Gerçek zamanlı örnekleme hızı (sample rate) ve bit derinliği (bit-depth) tanılaması ile **WASAPI Shared & Exclusive** çıkış aygıtı seçimi.
  * Ön yükselteç (Preamp) kontrolü, yerleşik odyofil eğrileri (*Harman Over-Ear* vb.) ve özel kullanıcı presetleri içeren **10-Bant Parametrik/Grafik Ekolayzer** (`32 Hz` – `16 kHz`).
  * Hi-Fi FLAC / Yüksek Kalite akış seçici ve ses seviyesi normalizasyonu.
* **DRM Uyumlu Oynatma Köprüsü:** Harici tarayıcı şişkinliği olmadan Widevine ve PlayReady DRM destekleyen doğrudan yerel `webview2-com` entegrasyonu.
* **Sosyal & Scrobbling Entegrasyonları:**
  * **Discord Rich Presence:** Gerçek zamanlı şarkı adı, sanatçı, albüm kapağı ve geçen/kalan süre senkronizasyonu.
  * **Last.fm Scrobbling:** Otomatik *Şu An Çalıyor (Now Playing)* güncellemeleri ve %50 veya 4 dakikalık dinleme eşiğinde otomatik scrobble gönderimi.
* **Tam Kitaplık & Sıra Yönetimi:**
  * Kalıcı arama geçmişi, pano desteği (`Ctrl+V` / `Ctrl+C` / `Ctrl+X`) ve sayfalı yükleme ile anında arama.
  * Beğenilen Şarkılar, Sanatçının Popüler Parçaları, Albüm Görünümleri ve tam Çalma Listesi yönetimi (oluşturma, silme, şarkı ekleme/çıkarma).
  * Karıştır (Shuffle), tekrar modları (`Kapalı` / `Liste` / `Tek`) ve tarayıcı tarzı İleri/Geri gezinme geçmişine sahip akıllı çalma sırası.
* **Uygulama İçi Güncelleme Denetleyicisi:** Arka planda otomatik sürüm kontrolü ve GitHub Releases üzerinden tek tıkla güncelleme indirme.
* **Çoklu Dil Desteği:** Otomatik sistem dili algılama ile Türkçe ve İngilizce yerelleştirme.

### 🛠️ Kişisel Derleme Talimatları (Windows)

#### Gereksinimler
1. **Microsoft Edge WebView2 Runtime** yüklü **Windows 10 / 11** (64-bit) (Windows 11'de ön yüklü gelir).
2. [rustup.rs](https://rustup.rs/) üzerinden **Rust Toolchain** (`stable-x86_64-pc-windows-msvc`).
3. *"C++ ile masaüstü geliştirme (Desktop development with C++)"* iş yükü ve Windows 10/11 SDK içeren **Visual Studio Build Tools 2022**.

#### Yerel Olarak Derleme
```bash
git clone https://github.com/egehansimarikk/nocturne.git
cd nocturne
cargo run --release
```

Derlenen çalıştırılabilir dosya kişisel kullanımınız için `target/release/nocturne.exe` konumunda oluşturulacaktır.

### ⌨️ Klavye Kısayolları

| Kısayol | İşlev |
| :--- | :--- |
| `Ctrl + F` | Arama çubuğuna odaklan |
| `Ctrl + V` / `C` / `X` | Arama veya pencere girişlerinde yapıştır, kopyala veya kes |
| `Space` | Oynat / Duraklat (arama kutusu odakta değilken) |
| `Ctrl + Sol / Sağ` | Önceki / Sonraki parça |
| `Alt + Sol / Sağ` | Görünüm geçmişinde Geri / İleri git |
| `M` | Sesi Kapat / Aç |
| `+` / `-` | Sesi Artır / Azalt |
| `Esc` | Açık pencereyi kapat veya arama odağından çık |

### 📜 Lisans & Telif Hakkı

**Telif Hakkı © 2026 Egehan Şımarık (`egehansimarikk`). Tüm Hakları Saklıdır.**

Nocturne, **Nocturne Özel Mülkiyet Kaynak Kodu Açık Lisansı (Nocturne Proprietary Source-Available License)** altında dağıtılmaktadır:
* **İzin Verilenler:** Kaynak kodu görüntülemek ve yalnızca kendi kişisel bilgisayarınızda özel, ticari olmayan kullanım amacıyla yerel olarak derlemek (`cargo build`).
* **Yasak Olanlar:** Kaynak kodun veya derlenmiş çalıştırılabilir dosyaların/kurulum sihirbazlarının yeniden dağıtılması, yüklenmesi, barındırılması veya paylaşılması; değiştirilmiş çatalların (fork) yayınlanması veya herhangi bir ticari kullanım.

Klonlamadan veya derlemeden önce tam [LICENSE](LICENSE) dosyasını okuyun.