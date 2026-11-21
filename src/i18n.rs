use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageSetting {
    #[default]
    System,
    Turkish,
    English,
    German,
    French,
    Spanish,
    Russian,
    Azerbaijani,
    Arabic,
    Japanese,
    Chinese,
}

impl LanguageSetting {
    pub const ALL: [Self; 11] = [
        Self::System,
        Self::Turkish,
        Self::English,
        Self::German,
        Self::French,
        Self::Spanish,
        Self::Russian,
        Self::Azerbaijani,
        Self::Arabic,
        Self::Japanese,
        Self::Chinese,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Sistem (Otomatik)",
            Self::Turkish => "Türkçe",
            Self::English => "English",
            Self::German => "Deutsch",
            Self::French => "Français",
            Self::Spanish => "Español",
            Self::Russian => "Русский",
            Self::Azerbaijani => "Azərbaycanca",
            Self::Arabic => "العربية",
            Self::Japanese => "日本語",
            Self::Chinese => "简体中文",
        }
    }

    fn locale_index(self) -> usize {
        match self {
            Self::System => system_locale_index(),
            Self::Turkish => 0,
            Self::English => 1,
            Self::German => 2,
            Self::French => 3,
            Self::Spanish => 4,
            Self::Russian => 5,
            Self::Azerbaijani => 6,
            Self::Arabic => 7,
            Self::Japanese => 8,
            Self::Chinese => 9,
        }
    }
}

#[cfg(windows)]
fn system_locale_index() -> usize {
    use windows::Win32::Globalization::GetUserDefaultUILanguage;
    let primary = unsafe { GetUserDefaultUILanguage() } & 0x03ff;
    match primary {
        0x1f => 0, // Turkish
        0x09 => 1, // English
        0x07 => 2, // German
        0x0c => 3, // French
        0x0a => 4, // Spanish
        0x19 => 5, // Russian
        0x2c => 6, // Azerbaijani
        0x01 => 7, // Arabic
        0x11 => 8, // Japanese
        0x04 => 9, // Chinese
        _ => 1,
    }
}

#[cfg(not(windows))]
fn system_locale_index() -> usize {
    let locale = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    if locale.starts_with("tr") {
        0
    } else if locale.starts_with("de") {
        2
    } else if locale.starts_with("fr") {
        3
    } else if locale.starts_with("es") {
        4
    } else if locale.starts_with("ru") {
        5
    } else if locale.starts_with("az") {
        6
    } else if locale.starts_with("ar") {
        7
    } else if locale.starts_with("ja") {
        8
    } else if locale.starts_with("zh") {
        9
    } else {
        1
    }
}

/// Translates UI copy. Keys are the Turkish source phrase; unknown keys fall back to it.
pub fn text(language: LanguageSetting, key: &str) -> String {
    let locale = language.locale_index();
    let translated = match key {
        "Görünüm" => ["Görünüm", "Appearance", "Darstellung", "Apparence", "Apariencia", "Вид", "Görünüş", "المظهر", "外観", "外观"],
        "Ses" => ["Ses", "Audio", "Audio", "Audio", "Sonido", "Звук", "Səs", "الصوت", "オーディオ", "音频"],
        "Ekolayzer" => ["Ekolayzer", "Equalizer", "Equalizer", "Égaliseur", "Ecualizador", "Эквалайзер", "Ekvalayzer", "المعادل", "イコライザー", "均衡器"],
        "Görünüm Teması" => ["Görünüm Teması", "Appearance theme", "Darstellungsthema", "Thème", "Tema visual", "Тема оформления", "Görünüş mövzusu", "نسق المظهر", "外観テーマ", "外观主题"],
        "Dil" => ["Dil", "Language", "Sprache", "Langue", "Idioma", "Язык", "Dil", "اللغة", "言語", "语言"],
        "Tema" => ["Tema", "Theme", "Thema", "Thème", "Tema", "Тема", "Mövzu", "السمة", "テーマ", "主题"],
        "Deezer Oturumu" => ["Deezer Oturumu", "Deezer login", "Deezer anmelden", "Connexion Deezer", "Sesión de Deezer", "Войти в Deezer", "Deezer sessiyası", "تسجيل الدخول إلى Deezer", "Deezerログイン", "Deezer 登录"],
        "Oturumu Gizle" => ["Oturumu Gizle", "Hide login", "Anmeldung ausblenden", "Masquer la connexion", "Ocultar sesión", "Скрыть вход", "Sessiyanı gizlət", "إخفاء تسجيل الدخول", "ログイン画面を隠す", "隐藏登录窗口"],
        "Çalma Listeleri" => ["Çalma Listeleri", "Playlists", "Playlists", "Playlists", "Playlists", "Плейлисты", "Pleylistlər", "قوائم التشغيل", "プレイリスト", "播放列表"],
        "Sistem (Otomatik)" => ["Sistem (Otomatik)", "System (Automatic)", "System (automatisch)", "Système (automatique)", "Sistema (automático)", "Системный (авто)", "Sistem (avtomatik)", "النظام (تلقائي)", "システム（自動）", "系统（自动）"],
        "Nocturne • Görünüm, Ses & Ekolayzer Ayarları" => ["Nocturne • Görünüm, Ses & Ekolayzer Ayarları", "Nocturne • Appearance, Audio & Equalizer", "Nocturne • Darstellung, Audio & Equalizer", "Nocturne • Apparence, audio et égaliseur", "Nocturne • Apariencia, audio y ecualizador", "Nocturne • Оформление, звук и эквалайзер", "Nocturne • Görünüş, Səs və Ekvalayzer Ayarları", "Nocturne • إعدادات المظهر والصوت والمعادل", "Nocturne • 外観・オーディオ・イコライザー設定", "Nocturne • 外观、音频与均衡器设置"],
        "GPUI temaları, çıkış donanımı, WASAPI ve 10-bant EQ" => ["GPUI temaları, çıkış donanımı, WASAPI ve 10-bant EQ", "GPUI themes, output device, WASAPI, and 10-band EQ", "GPUI-Themes, Ausgabegerät, WASAPI und 10-Band-EQ", "Thèmes GPUI, périphérique de sortie, WASAPI et égaliseur 10 bandes", "Temas GPUI, dispositivo de salida, WASAPI y ecualizador de 10 bandas", "Темы GPUI, устройство вывода, WASAPI и 10-полосный эквалайзер", "GPUI mövzuları, çıxış cihazı, WASAPI və 10-zolaqlı EQ", "سمات GPUI وجهاز الإخراج وWASAPI ومعادل 10 نطاقات", "GPUIテーマ、出力デバイス、WASAPI、10バンドEQ", "GPUI 主题、输出设备、WASAPI 与 10 段均衡器"],
        "Kapat (ESC)" => ["Kapat (ESC)", "Close (ESC)", "Schließen (ESC)", "Fermer (Échap)", "Cerrar (ESC)", "Закрыть (ESC)", "Bağla (ESC)", "إغلاق (ESC)", "閉じる (ESC)", "关闭 (ESC)"],
        "Kapat" => ["Kapat", "Close", "Schließen", "Fermer", "Cerrar", "Закрыть", "Bağla", "إغلاق", "閉じる", "关闭"],
        "1. Ses Çıkış Aygıtı (DAC / Kulaklık / Hoparlör)" => ["1. Ses Çıkış Aygıtı (DAC / Kulaklık / Hoparlör)", "1. Audio Output Device (DAC / Headphones / Speakers)", "1. Audioausgabegerät (DAC / Kopfhörer / Lautsprecher)", "1. Périphérique de sortie audio (DAC / Casque / Haut-parleurs)", "1. Dispositivo de salida de audio (DAC / Auriculares / Altavoces)", "1. Устройство вывода звука (ЦАП / Наушники / Динамики)", "1. Səs Çıxış Cihazı (DAC / Qulaqlıq / Dinamik)", "1. جهاز إخراج الصوت (DAC / سماعات / مكبرات صوت)", "1. オーディオ出力デバイス（DAC / ヘッドホン / スピーカー）", "1. 音频输出设备（DAC / 耳机 / 扬声器）"],
        "Aygıtları Yenile" => ["Aygıtları Yenile", "Refresh Devices", "Geräte aktualisieren", "Actualiser les périphériques", "Actualizar dispositivos", "Обновить устройства", "Cihazları Yenilə", "تحديث الأجهزة", "デバイスを更新", "刷新设备"],
        "Aygıt seçimi Exclusive çıkışta kullanılır; Shared mod Windows'un varsayılan aygıtını izler." => ["Aygıt seçimi Exclusive çıkışta kullanılır; Shared mod Windows'un varsayılan aygıtını izler.", "Device selection applies to Exclusive output; Shared mode follows Windows' default device.", "Die Geräteauswahl gilt für die exklusive Ausgabe; der geteilte Modus folgt dem Windows-Standardgerät.", "La sélection de périphérique s'applique à la sortie exclusive ; le mode partagé suit le périphérique Windows par défaut.", "La selección de dispositivo se usa en salida exclusiva; el modo compartido sigue el dispositivo predeterminado de Windows.", "Выбор устройства применяется в монопольном режиме; общий режим использует устройство Windows по умолчанию.", "Cihaz seçimi Exclusive çıxışda istifadə olunur; Shared rejim Windows-un standart cihazını izləyir.", "يُستخدم اختيار الجهاز في الإخراج الحصري؛ ويتبع الوضع المشترك جهاز Windows الافتراضي.", "デバイス選択は排他出力で使用され、共有モードはWindowsの既定デバイスに従います。", "设备选择仅用于独占输出；共享模式跟随 Windows 默认设备。"],
        "Windows Varsayılan Çıkış Aygıtı (Otomatik)" => ["Windows Varsayılan Çıkış Aygıtı (Otomatik)", "Windows Default Output Device (Automatic)", "Windows-Standardausgabegerät (automatisch)", "Périphérique de sortie Windows par défaut (automatique)", "Dispositivo de salida predeterminado de Windows (automático)", "Устройство вывода Windows по умолчанию (авто)", "Windows Standart Çıxış Cihazı (Avtomatik)", "جهاز إخراج Windows الافتراضي (تلقائي)", "Windows 既定の出力デバイス（自動）", "Windows 默认输出设备（自动）"],
        "Sistem Varsayılanı" => ["Sistem Varsayılanı", "System Default", "Systemstandard", "Par défaut du système", "Predeterminado del sistema", "По умолчанию", "Sistem standartı", "افتراضي النظام", "システム既定", "系统默认"],
        "Seçili" => ["Seçili", "Selected", "Ausgewählt", "Sélectionné", "Seleccionado", "Выбрано", "Seçilib", "محدد", "選択中", "已选"],
        "2. WASAPI Çıkış Modu & Donanım Doğrulama" => ["2. WASAPI Çıkış Modu & Donanım Doğrulama", "2. WASAPI Output Mode & Hardware Verification", "2. WASAPI-Ausgabemodus & Hardware-Verifizierung", "2. Mode de sortie WASAPI et vérification matérielle", "2. Modo de salida WASAPI y verificación de hardware", "2. Режим вывода WASAPI и проверка оборудования", "2. WASAPI Çıxış Rejimi və Avadanlıq Doğrulaması", "2. وضع إخراج WASAPI والتحقق من العتاد", "2. WASAPI 出力モードとハードウェア検証", "2. WASAPI 输出模式与硬件验证"],
        "WASAPI Shared" => ["WASAPI Shared", "WASAPI Shared", "WASAPI Shared", "WASAPI partagé", "WASAPI compartido", "WASAPI Shared", "WASAPI Shared", "WASAPI مشترك", "WASAPI 共有", "WASAPI 共享模式"],
        "Diğer uygulamalarla birlikte Windows'un varsayılan çıkışını kullanır." => ["Diğer uygulamalarla birlikte Windows'un varsayılan çıkışını kullanır.", "Shares Windows' default audio output alongside other apps.", "Teilt die Windows-Standardausgabe mit anderen Anwendungen.", "Partage la sortie audio par défaut de Windows avec d'autres applications.", "Comparte la salida de audio predeterminada de Windows con otras aplicaciones.", "Использует общий вывод звука Windows вместе с другими приложениями.", "Digər tətbiqlərlə birlikdə Windows-un standart çıxışını istifadə edir.", "يستخدم إخراج الصوت الافتراضي لنظام Windows مع التطبيقات الأخرى.", "他のアプリとWindowsの既定オーディオ出力を共有します。", "与其他应用共享 Windows 默认音频输出。"],
        "WASAPI Exclusive (Özel çıkış)" => ["WASAPI Exclusive (Özel çıkış)", "WASAPI Exclusive (Direct Output)", "WASAPI Exclusive (Direktausgabe)", "WASAPI exclusif (sortie directe)", "WASAPI exclusivo (salida directa)", "WASAPI Exclusive (прямой вывод)", "WASAPI Exclusive (Xüsusi çıxış)", "WASAPI حصري (إخراج مباشر)", "WASAPI 排他（ダイレクト出力）", "WASAPI 独占模式（直通输出）"],
        "WebView2 ses kaynağı bu modu desteklemediği için şu an kullanılamıyor." => ["WebView2 ses kaynağı bu modu desteklemediği için şu an kullanılamıyor.", "Currently unavailable because the WebView2 audio source does not support this mode.", "Derzeit nicht verfügbar, da die WebView2-Audioquelle diesen Modus nicht unterstützt.", "Actuellement indisponible car la source audio WebView2 ne prend pas en charge ce mode.", "No disponible actualmente porque la fuente de audio de WebView2 no admite este modo.", "Сейчас недоступно, так как источник звука WebView2 не поддерживает этот режим.", "WebView2 səs mənbəyi bu rejimi dəstəkləmədiyi üçün hazırda əlçatan deyil.", "غير متاح حاليًا لأن مصدر صوت WebView2 لا يدعم هذا الوضع.", "WebView2 オーディオソースがこのモードに対応していないため現在利用できません。", "由于 WebView2 音频源不支持此模式，当前不可用。"],
        "Aktif Donanım" => ["Aktif Donanım", "Active Hardware", "Aktive Hardware", "Matériel actif", "Hardware activo", "Активное устройство", "Aktiv Avadanlıq", "العتاد النشط", "有効なハードウェア", "当前硬件"],
        "Kanal" => ["Kanal", "Channels", "Kanäle", "canaux", "canales", "кан.", "Kanal", "قنوات", "チャンネル", "声道"],
        "16-Bant Spektrum" => ["16-Bant Spektrum", "16-Band Spectrum", "16-Band-Spektrum", "Spectre 16 bandes", "Espectro de 16 bandas", "16-полосный спектр", "16-Zolaqlı Spektr", "طيف 16 نطاقًا", "16バンドスペクトラム", "16 段频谱"],
        "Kapalı" => ["Kapalı", "Off", "Aus", "Désactivé", "Desactivado", "Выкл.", "Bağlı", "متوقف", "オフ", "关闭"],
        "İlk parçadan itibaren ses akışına uygulanıyor" => ["İlk parçadan itibaren ses akışına uygulanıyor", "Applied to the audio stream from the very first track", "Wird ab dem ersten Titel auf den Audiostream angewendet", "Appliqué au flux audio dès le premier morceau", "Aplicado al flujo de audio desde la primera pista", "Применяется к аудиопотоку с самого первого трека", "İlk trekdən etibarən səs axınına tətbiq olunur", "يُطبَّق على تدفق الصوت منذ المقطع الأول", "最初の曲からオーディオストリームに適用中", "从第一首曲目起即应用于音频流"],
        "Açık, WebView2 ses zincirine bağlanıyor" => ["Açık, WebView2 ses zincirine bağlanıyor", "Enabled, attaching to WebView2 audio chain", "Aktiviert, wird mit der WebView2-Audiokette verbunden", "Activé, connexion à la chaîne audio WebView2", "Activado, conectando a la cadena de audio de WebView2", "Включён, подключение к аудиоцепи WebView2", "Aktivdir, WebView2 səs zəncirinə qoşulur", "مفعّل، جارٍ الاتصال بسلسلة صوت WebView2", "有効、WebView2 オーディオチェーンに接続中", "已启用，正在连接到 WebView2 音频链"],
        "WebView2 analiz zinciri" => ["WebView2 analiz zinciri", "WebView2 analyser chain", "WebView2-Analysekette", "Chaîne d'analyse WebView2", "Cadena de análisis WebView2", "Цепь анализа WebView2", "WebView2 analiz zənciri", "سلسلة تحليل WebView2", "WebView2 アナライザーチェーン", "WebView2 分析链"],
        "WASAPI süreç loopback" => ["WASAPI süreç loopback", "WASAPI process loopback", "WASAPI-Prozess-Loopback", "Bouclage de processus WASAPI", "Bucle invertido de proceso WASAPI", "Loopback процесса WASAPI", "WASAPI proses loopback", "استرجاع عملية WASAPI", "WASAPI プロセスループバック", "WASAPI 进程回环"],
        "3. 10-Bant Grafik Ekolayzer (BiQuad)" => ["3. 10-Bant Grafik Ekolayzer (BiQuad)", "3. 10-Band Graphic Equalizer (BiQuad)", "3. Grafischer 10-Band-Equalizer (BiQuad)", "3. Égaliseur graphique 10 bandes (BiQuad)", "3. Ecualizador gráfico de 10 bandas (BiQuad)", "3. 10-полосный графический эквалайзер (BiQuad)", "3. 10-Zolaqlı Qrafik Ekvalayzer (BiQuad)", "3. معادل رسومي 10 نطاقات (BiQuad)", "3. 10バンドグラフィックイコライザー（BiQuad）", "3. 10 段图示均衡器（BiQuad）"],
        "EQ Aktif" => ["EQ Aktif", "EQ Active", "EQ aktiv", "Égaliseur actif", "EQ activo", "Эквалайзер активен", "EQ Aktivdir", "المعادل نشط", "EQ 有効", "均衡器已启用"],
        "Kapalı (Bypass)" => ["Kapalı (Bypass)", "Off (Bypass)", "Aus (Bypass)", "Désactivé (Bypass)", "Desactivado (Bypass)", "Выкл. (обход)", "Bağlı (Bypass)", "متوقف (تجاوز)", "オフ（バイパス）", "关闭（直通）"],
        "Ön Yükselteç (Preamp)" => ["Ön Yükselteç (Preamp)", "Preamp", "Vorverstärker (Preamp)", "Préampli", "Preamplificador", "Предусилитель (Preamp)", "Ön Gücləndirici (Preamp)", "مضخم أولي (Preamp)", "プリアンプ", "前置增益（Preamp）"],
        "-12.0 dB … +6.0 dB • 0.5 dB adım" => ["-12.0 dB … +6.0 dB • 0.5 dB adım", "-12.0 dB … +6.0 dB • 0.5 dB step", "-12,0 dB … +6,0 dB • 0,5-dB-Schritte", "-12,0 dB … +6,0 dB • pas de 0,5 dB", "-12,0 dB … +6,0 dB • paso de 0,5 dB", "-12,0 дБ … +6,0 дБ • шаг 0,5 дБ", "-12.0 dB … +6.0 dB • 0.5 dB addım", "-12.0 dB … +6.0 dB • خطوة 0.5 dB", "-12.0 dB 〜 +6.0 dB • 0.5 dB 刻み", "-12.0 dB … +6.0 dB • 0.5 dB 步进"],
        "Özel EQ Presetleri" => ["Özel EQ Presetleri", "Custom EQ Presets", "Eigene EQ-Presets", "Préréglages EQ personnalisés", "Preajustes de EQ personalizados", "Пользовательские пресеты EQ", "Xüsusi EQ Presetləri", "إعدادات EQ المخصصة", "カスタムEQプリセット", "自定义均衡器预设"],
        "Henüz kayıtlı preset yok." => ["Henüz kayıtlı preset yok.", "No saved presets yet.", "Noch keine gespeicherten Presets.", "Aucun préréglage enregistré.", "Aún no hay preajustes guardados.", "Сохранённых пресетов пока нет.", "Hələ yadda saxlanmış preset yoxdur.", "لا توجد إعدادات مسبقة محفوظة بعد.", "保存されたプリセットはまだありません。", "暂无已保存的预设。"],
        "Preset adı yaz, Enter ile kaydet" => ["Preset adı yaz, Enter ile kaydet", "Type preset name, press Enter to save", "Preset-Namen eingeben, mit Enter speichern", "Saisir le nom du préréglage, Entrée pour enregistrer", "Escribe el nombre y pulsa Enter para guardar", "Введите имя пресета и нажмите Enter", "Preset adını yazın, Enter ilə yadda saxlayın", "اكتب اسم الإعداد المسبق واضغط Enter للحفظ", "プリセット名を入力して Enter で保存", "输入预设名称，按 Enter 保存"],
        "Preset Kaydet" => ["Preset Kaydet", "Save Preset", "Preset speichern", "Enregistrer", "Guardar preajuste", "Сохранить пресет", "Preseti Yadda Saxla", "حفظ الإعداد", "プリセット保存", "保存预设"],
        "Özel" => ["Özel", "Custom", "Eigene", "Personnalisé", "Personalizado", "Свой", "Xüsusi", "مخصص", "カスタム", "自定义"],
        "Preset kaydedildi" => ["Preset kaydedildi", "Preset saved", "Preset gespeichert", "Préréglage enregistré", "Preajuste guardado", "Пресет сохранён", "Preset yadda saxlanıldı", "تم حفظ الإعداد المسبق", "プリセットを保存しました", "预设已保存"],
        "Tekrar" => ["Tekrar", "Repeat", "Wiederholen", "Répéter", "Repetir", "Повтор", "Təkrar", "تكرار", "リピート", "循环"],
        "Liste" => ["Liste", "All", "Alle", "Tout", "Todo", "Все", "Hamısı", "الكل", "全曲", "列表"],
        "Tek" => ["Tek", "One", "Einzeln", "Un titre", "Una", "Один", "Tək", "واحدة", "1曲", "单曲"],
        "Sessiz" => ["Sessiz", "Muted", "Stumm", "Muet", "Silenciado", "Без звука", "Səssiz", "مكتوم", "ミュート", "静音"],
        "Keşfet" => ["Keşfet", "Discover", "Entdecken", "Découvrir", "Descubrir", "Обзор", "Kəşf et", "اكتشف", "探索", "发现"],
        "Arama:" => ["Arama:", "Search:", "Suche:", "Recherche :", "Búsqueda:", "Поиск:", "Axtarış:", "بحث:", "検索:", "搜索："],
        "ARAMA GEÇMİŞİ" => ["ARAMA GEÇMİŞİ", "SEARCH HISTORY", "SUCHVERLAUF", "HISTORIQUE", "HISTORIAL DE BÚSQUEDA", "ИСТОРИЯ ПОИСКА", "AXTARIŞ TARİXÇƏSİ", "سجل البحث", "検索履歴", "搜索历史"],
        "Sanatçı, şarkı veya albüm ara (Enter)..." => ["Sanatçı, şarkı veya albüm ara (Enter)...", "Search artist, song, or album (Enter)...", "Künstler, Titel oder Album suchen (Enter)...", "Rechercher un artiste, titre ou album (Entrée)...", "Buscar artista, canción o álbum (Enter)...", "Поиск исполнителя, трека или альбома (Enter)...", "İfaçı, mahnı və ya albom axtar (Enter)...", "ابحث عن فنان أو أغنية أو ألبوم (Enter)...", "アーティスト・曲・アルバムを検索 (Enter)...", "搜索艺人、歌曲或专辑（按 Enter）..."],
        "Sıra" => ["Sıra", "Queue", "Warteschlange", "File", "Cola", "Очередь", "Növbə", "قائمة الانتظار", "キュー", "队列"],
        "Ayarlar" => ["Ayarlar", "Settings", "Einstellungen", "Paramètres", "Ajustes", "Настройки", "Ayarlar", "الإعدادات", "設定", "设置"],
        "KİTAPLIĞIM" => ["KİTAPLIĞIM", "MY LIBRARY", "MEINE BIBLIOTHEK", "MA BIBLIOTHÈQUE", "MI BIBLIOTECA", "МОЯ МЕДИАТЕКА", "KİTABXANAM", "مكتبتي", "マイライブラリ", "我的音乐库"],
        "Beğenilen Şarkılar" => ["Beğenilen Şarkılar", "Favorite Tracks", "Lieblingstitel", "Coups de cœur", "Canciones favoritas", "Любимые треки", "Bəyənilən Mahnılar", "الأغاني المفضلة", "お気に入りの曲", "喜欢的歌曲"],
        "HIZLI KATEGORİLER" => ["HIZLI KATEGORİLER", "QUICK CATEGORIES", "SCHNELLKATEGORIEN", "CATÉGORIES RAPIDES", "CATEGORÍAS RÁPIDAS", "БЫСТРЫЕ КАТЕГОРИИ", "SÜRƏTLİ KATEQORİYALAR", "فئات سريعة", "クイックカテゴリー", "快捷分类"],
        "ÇALMA LİSTELERİM" => ["ÇALMA LİSTELERİM", "MY PLAYLISTS", "MEINE PLAYLISTS", "MES PLAYLISTS", "MIS PLAYLISTS", "МОИ ПЛЕЙЛИСТЫ", "PLEYLISTLƏRİM", "قوائم التشغيل", "マイプレイリスト", "我的播放列表"],
        "Yeni" => ["Yeni", "New", "Neu", "Nouveau", "Nueva", "Новый", "Yeni", "جديد", "新規", "新建"],
        "Yenile" => ["Yenile", "Refresh", "Aktualisieren", "Actualiser", "Actualizar", "Обновить", "Yenilə", "تحديث", "更新", "刷新"],
        "Sil?" => ["Sil?", "Delete?", "Löschen?", "Supprimer ?", "¿Eliminar?", "Удалить?", "Silinsin?", "حذف؟", "削除？", "删除？"],
        "Çalma Listesi" => ["Çalma Listesi", "Playlist", "Playlist", "Playlist", "Playlist", "Плейлист", "Pleylist", "قائمة تشغيل", "プレイリスト", "播放列表"],
        "Sanatçı" => ["Sanatçı", "Artist", "Künstler", "Artiste", "Artista", "Исполнитель", "İfaçı", "فنان", "アーティスト", "艺人"],
        "Albüm" => ["Albüm", "Album", "Album", "Album", "Álbum", "Альбом", "Albom", "ألبوم", "アルバム", "专辑"],
        "Tümünü Çal" => ["Tümünü Çal", "Play All", "Alle abspielen", "Tout lire", "Reproducir todo", "Слушать всё", "Hamısını Oxut", "تشغيل الكل", "すべて再生", "播放全部"],
        "Karışık Çal" => ["Karışık Çal", "Shuffle Play", "Zufallswiedergabe", "Lecture aléatoire", "Aleatorio", "Перемешать", "Qarışıq Oxut", "تشغيل عشوائي", "シャッフル再生", "随机播放"],
        "Yükleniyor..." => ["Yükleniyor...", "Loading...", "Wird geladen...", "Chargement...", "Cargando...", "Загрузка...", "Yüklənir...", "جارٍ التحميل...", "読み込み中...", "加载中..."],
        "Daha Fazla Yükle" => ["Daha Fazla Yükle", "Load More", "Mehr laden", "Charger plus", "Cargar más", "Загрузить ещё", "Daha Çox Yüklə", "تحميل المزيد", "さらに読み込む", "加载更多"],
        "Çalma Sırası" => ["Çalma Sırası", "Play Queue", "Wiedergabeliste", "File de lecture", "Cola de reproducción", "Очередь воспроизведения", "Oxuma Növbəsi", "قائمة التشغيل التالية", "再生キュー", "播放队列"],
        "Temizle" => ["Temizle", "Clear", "Leeren", "Effacer", "Limpiar", "Очистить", "Təmizlə", "مسح", "クリア", "清空"],
        "Kapak" => ["Kapak", "Cover", "Cover", "Pochette", "Carátula", "Обложка", "Qapaq", "الغلاف", "カバー", "封面"],
        "Parça Seçilmedi" => ["Parça Seçilmedi", "No Track Selected", "Kein Titel ausgewählt", "Aucun titre sélectionné", "Ninguna pista seleccionada", "Трек не выбран", "Mahnı Seçilməyib", "لم يتم اختيار مقطع", "曲が選択されていません", "未选择曲目"],
        "Listeden bir şarkıya tıkla veya arama yap" => ["Listeden bir şarkıya tıkla veya arama yap", "Click a track from the list or search", "Klicke auf einen Titel oder starte eine Suche", "Cliquez sur un titre ou lancez une recherche", "Haz clic en una pista o busca", "Выберите трек из списка или воспользуйтесь поиском", "Siyahıdan mahnı seçin və ya axtarış edin", "انقر على أغنية من القائمة أو ابحث", "リストから曲を選ぶか検索してください", "从列表中点击歌曲或进行搜索"],
        "Karıştır" => ["Karıştır", "Shuffle", "Zufall", "Aléatoire", "Aleatorio", "Перемешать", "Qarışdır", "عشوائي", "シャッフル", "随机"],
        "Yeni çalma listesi" => ["Yeni çalma listesi", "New playlist", "Neue Playlist", "Nouvelle playlist", "Nueva playlist", "Новый плейлист", "Yeni pleylist", "قائمة تشغيل جديدة", "新規プレイリスト", "新建播放列表"],
        "Çalma listesi adı" => ["Çalma listesi adı", "Playlist name", "Playlist-Name", "Nom de la playlist", "Nombre de la playlist", "Название плейлиста", "Pleylist adı", "اسم قائمة التشغيل", "プレイリスト名", "播放列表名称"],
        "Liste oluşturulunca çalan şarkı eklenecek. Enter ile onayla, Esc ile kapat." => ["Liste oluşturulunca çalan şarkı eklenecek. Enter ile onayla, Esc ile kapat.", "The selected track will be added once created. Enter to confirm, Esc to close.", "Der gewählte Titel wird hinzugefügt. Enter zum Bestätigen, Esc zum Schließen.", "Le titre sélectionné sera ajouté. Entrée pour confirmer, Échap pour fermer.", "La pista se añadirá al crearla. Enter para confirmar, Esc para cerrar.", "Выбранный трек будет добавлен. Enter — подтвердить, Esc — закрыть.", "Seçilmiş mahnı əlavə olunacaq. Enter ilə təsdiqləyin, Esc ilə bağlayın.", "ستُضاف الأغنية المختارة بعد الإنشاء. Enter للتأكيد، Esc للإغلاق.", "作成後に選択した曲が追加されます。Enter で確定、Esc で閉じる。", "创建后将自动添加所选歌曲。按 Enter 确认，Esc 关闭。"],
        "Enter ile oluştur, Esc ile kapat." => ["Enter ile oluştur, Esc ile kapat.", "Press Enter to create, Esc to close.", "Enter zum Erstellen, Esc zum Schließen.", "Entrée pour créer, Échap pour fermer.", "Enter para crear, Esc para cerrar.", "Enter — создать, Esc — закрыть.", "Yaratmaq üçün Enter, bağlamaq üçün Esc basın.", "اضغط Enter للإنشاء، وEsc للإغلاق.", "Enter で作成、Esc で閉じます。", "按 Enter 创建，Esc 关闭。"],
        "İptal" => ["İptal", "Cancel", "Abbrechen", "Annuler", "Cancelar", "Отмена", "Ləğv et", "إلغاء", "キャンセル", "取消"],
        "Oluştur" => ["Oluştur", "Create", "Erstellen", "Créer", "Crear", "Создать", "Yarat", "إنشاء", "作成", "创建"],
        "Yeni çalma listesi oluştur" => ["Yeni çalma listesi oluştur", "Create new playlist", "Neue Playlist erstellen", "Créer une nouvelle playlist", "Crear nueva playlist", "Создать новый плейлист", "Yeni pleylist yarat", "إنشاء قائمة تشغيل جديدة", "新しいプレイリストを作成", "创建新播放列表"],
        "Şarkıyı çalma listesine ekle" => ["Şarkıyı çalma listesine ekle", "Add track to playlist", "Titel zur Playlist hinzufügen", "Ajouter le titre à la playlist", "Añadir pista a la playlist", "Добавить трек в плейлист", "Mahnını pleylistə əlavə et", "إضافة الأغنية إلى قائمة تشغيل", "曲をプレイリストに追加", "将歌曲添加到播放列表"],
        "Şarkıyı çalma listesinden çıkar" => ["Şarkıyı çalma listesinden çıkar", "Remove track from playlist", "Titel aus Playlist entfernen", "Retirer le titre de la playlist", "Quitar pista de la playlist", "Удалить трек из плейлиста", "Mahnını pleylistdən çıxar", "إزالة الأغنية من قائمة التشغيل", "曲をプレイリストから削除", "将歌曲从播放列表中移除"],
        "Çalma listesi bulunamadı. Deezer oturumunu açıp tekrar dene." => ["Çalma listesi bulunamadı. Deezer oturumunu açıp tekrar dene.", "No playlists found. Sign in to Deezer and try again.", "Keine Playlists gefunden. Bei Deezer anmelden und erneut versuchen.", "Aucune playlist trouvée. Connectez-vous à Deezer et réessayez.", "No se encontraron playlists. Inicia sesión en Deezer e inténtalo de nuevo.", "Плейлисты не найдены. Войдите в Deezer и повторите попытку.", "Pleylist tapılmadı. Deezer hesabına daxil olub yenidən yoxlayın.", "لم يتم العثور على قوائم تشغيل. سجّل الدخول إلى Deezer وحاول مرة أخرى.", "プレイリストが見つかりません。Deezer にログインして再試行してください。", "未找到播放列表。请登录 Deezer 后重试。"],
        "Bu şarkı zaten beğenilenlerde." => ["Bu şarkı zaten beğenilenlerde.", "This track is already in your favorites.", "Dieser Titel ist bereits in deinen Favoriten.", "Ce titre est déjà dans vos favoris.", "Esta pista ya está en tus favoritos.", "Этот трек уже в любимых.", "Bu mahnı artıq bəyənilənlərdədir.", "هذه الأغنية موجودة بالفعل في المفضلة.", "この曲はすでにお気に入りに登録されています。", "这首歌曲已在你喜欢的列表中。"],
        "Beğenilenlere eklemek için önce Deezer oturumu aç." => ["Beğenilenlere eklemek için önce Deezer oturumu aç.", "Sign in to Deezer first to add favorites.", "Melde dich zuerst bei Deezer an, um Favoriten hinzuzufügen.", "Connectez-vous d'abord à Deezer pour ajouter des favoris.", "Inicia sesión en Deezer primero para añadir favoritos.", "Сначала войдите в Deezer, чтобы добавлять треки в любимые.", "Bəyənilənlərə əlavə etmək üçün əvvəlcə Deezer hesabına daxil olun.", "سجّل الدخول إلى Deezer أولاً لإضافة الأغاني إلى المفضلة.", "お気に入りに追加するには先に Deezer にログインしてください。", "请先登录 Deezer 再添加到喜欢的歌曲。"],
        "Çalma listesine eklemek için önce Deezer oturumu aç." => ["Çalma listesine eklemek için önce Deezer oturumu aç.", "Sign in to Deezer first to add to a playlist.", "Melde dich zuerst bei Deezer an, um zur Playlist hinzuzufügen.", "Connectez-vous d'abord à Deezer pour ajouter à une playlist.", "Inicia sesión en Deezer primero para añadir a una playlist.", "Сначала войдите в Deezer, чтобы добавить трек в плейлист.", "Pleylistə əlavə etmək üçün əvvəlcə Deezer hesabına daxil olun.", "سجّل الدخول إلى Deezer أولاً للإضافة إلى قائمة تشغيل.", "プレイリストに追加するには先に Deezer にログインしてください。", "请先登录 Deezer 再添加到播放列表。"],
        "Şarkı beğenilenlere eklendi." => ["Şarkı beğenilenlere eklendi.", "Track added to favorites.", "Titel zu Favoriten hinzugefügt.", "Titre ajouté aux favoris.", "Pista añadida a favoritos.", "Трек добавлен в любимые.", "Mahnı bəyənilənlərə əlavə edildi.", "تمت إضافة الأغنية إلى المفضلة.", "お気に入りに追加しました。", "已添加到喜欢的歌曲。"],
        "Şarkı beğenilenlerden çıkarıldı." => ["Şarkı beğenilenlerden çıkarıldı.", "Track removed from favorites.", "Titel aus Favoriten entfernt.", "Titre retiré des favoris.", "Pista eliminada de favoritos.", "Трек удалён из любимых.", "Mahnı bəyənilənlərdən çıxarıldı.", "تمت إزالة الأغنية من المفضلة.", "お気に入りから削除しました。", "已从喜欢的歌曲中移除。"],
        "Şarkı çalma listesine eklendi." => ["Şarkı çalma listesine eklendi.", "Track added to playlist.", "Titel zur Playlist hinzugefügt.", "Titre ajouté à la playlist.", "Pista añadida a la playlist.", "Трек добавлен в плейлист.", "Mahnı pleylistə əlavə edildi.", "تمت إضافة الأغنية إلى قائمة التشغيل.", "プレイリストに追加しました。", "已添加到播放列表。"],
        "Şarkı çalma listesinden çıkarıldı." => ["Şarkı çalma listesinden çıkarıldı.", "Track removed from playlist.", "Titel aus Playlist entfernt.", "Titre retiré de la playlist.", "Pista eliminada de la playlist.", "Трек удалён из плейлиста.", "Mahnı pleylistdən çıxarıldı.", "تمت إزالة الأغنية من قائمة التشغيل.", "プレイリストから削除しました。", "已从播放列表中移除。"],
        "Çalma listesi oluşturuluyor..." => ["Çalma listesi oluşturuluyor...", "Creating playlist...", "Playlist wird erstellt...", "Création de la playlist...", "Creando playlist...", "Создание плейлиста...", "Pleylist yaradılır...", "جارٍ إنشاء قائمة التشغيل...", "プレイリストを作成中...", "正在创建播放列表..."],
        "Çalma listesi oluşturuldu." => ["Çalma listesi oluşturuldu.", "Playlist created.", "Playlist erstellt.", "Playlist créée.", "Playlist creada.", "Плейлист создан.", "Pleylist yaradıldı.", "تم إنشاء قائمة التشغيل.", "プレイリストを作成しました。", "播放列表已创建。"],
        "Çalma listesi oluşturulamadı." => ["Çalma listesi oluşturulamadı.", "Could not create playlist.", "Playlist konnte nicht erstellt werden.", "Impossible de créer la playlist.", "No se pudo crear la playlist.", "Не удалось создать плейлист.", "Pleylist yaradıla bilmədi.", "تعذر إنشاء قائمة التشغيل.", "プレイリストを作成できませんでした。", "无法创建播放列表。"],
        "Çalma listesi siliniyor..." => ["Çalma listesi siliniyor...", "Deleting playlist...", "Playlist wird gelöscht...", "Suppression de la playlist...", "Eliminando playlist...", "Удаление плейлиста...", "Pleylist silinir...", "جارٍ حذف قائمة التشغيل...", "プレイリストを削除中...", "正在删除播放列表..."],
        "Çalma listesi silindi." => ["Çalma listesi silindi.", "Playlist deleted.", "Playlist gelöscht.", "Playlist supprimée.", "Playlist eliminada.", "Плейлист удалён.", "Pleylist silindi.", "تم حذف قائمة التشغيل.", "プレイリストを削除しました。", "播放列表已删除。"],
        "Çalma listesi adı boş olamaz." => ["Çalma listesi adı boş olamaz.", "Playlist name cannot be empty.", "Der Playlist-Name darf nicht leer sein.", "Le nom de la playlist ne peut pas être vide.", "El nombre de la playlist no puede estar vacío.", "Название плейлиста не может быть пустым.", "Pleylist adı boş ola bilməz.", "لا يمكن أن يكون اسم قائمة التشغيل فارغًا.", "プレイリスト名を入力してください。", "播放列表名称不能为空。"],
        "Hata:" => ["Hata:", "Error:", "Fehler:", "Erreur :", "Error:", "Ошибка:", "Xəta:", "خطأ:", "エラー:", "错误："],
        "Drift Phonk" => ["Drift Phonk", "Drift Phonk", "Drift Phonk", "Drift Phonk", "Drift Phonk", "Дрифт-фонк", "Drift Phonk", "دريفت فونك", "ドリフト・フォンク", "漂移 Phonk"],
        "Caucasian & Russian" => ["Caucasian & Russian", "Caucasian & Russian", "Kaukasisch & Russisch", "Caucase & Russie", "Cáucaso y Rusia", "Кавказ и Россия", "Qafqaz və Rus", "قوقازي وروسي", "コーカサス＆ロシア", "高加索与俄语"],
        "Dark Pop & Trap" => ["Dark Pop & Trap", "Dark Pop & Trap", "Dark Pop & Trap", "Dark Pop & Trap", "Dark Pop y Trap", "Дарк-поп и трэп", "Dark Pop və Trap", "دارك بوب وتراب", "ダークポップ＆トラップ", "暗黑流行与陷阱"],
        "Hi-Fi / Audiophile Test" => ["Hi-Fi / Audiophile Test", "Hi-Fi / Audiophile Test", "Hi-Fi- / Audiophilen-Test", "Test Hi-Fi / Audiophile", "Prueba Hi-Fi / Audiófilo", "Hi-Fi / Аудиофильский тест", "Hi-Fi / Audiofil Testi", "اختبار Hi-Fi / عشاق الصوت", "Hi-Fi / オーディオファンテスト", "Hi-Fi / 发烧试音"],
        "Türkçe Hip-Hop" => ["Türkçe Hip-Hop", "Turkish Hip-Hop", "Türkischer Hip-Hop", "Hip-hop turc", "Hip-hop turco", "Турецкий хип-хоп", "Türk Hip-Hop", "هيب هوب تركي", "トルコ語ヒップホップ", "土耳其嘻哈"],
        _ => return key.to_string(),
    };
    translated.get(locale).copied().unwrap_or(key).to_string()
}