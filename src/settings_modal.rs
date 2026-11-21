use crate::NocturneApp;
use crate::audio_engine::OutputShareMode;
use crate::i18n::{LanguageSetting, text};
use crate::icons::lucide_icon;
use crate::integrations::UpdateCheckState;
use crate::theme::{ThemeId, ThemePalette};
use crate::webview_bridge::AudioQuality;
use gpui::{
    Context, ElementId, InteractiveElement, IntoElement, ParentElement, Styled, div, prelude::*,
    px, rgb, rgba,
};

const EQ_FREQS: [&str; 10] = [
    "32", "64", "125", "250", "500", "1K", "2K", "4K", "8K", "16K",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Audio,
    Equalizer,
    Connections,
}

impl SettingsTab {
    const ALL: [Self; 4] = [
        Self::Appearance,
        Self::Audio,
        Self::Equalizer,
        Self::Connections,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::Appearance => "Görünüm",
            Self::Audio => "Ses",
            Self::Equalizer => "Ekolayzer",
            Self::Connections => "Bağlantılar",
        }
    }

    fn icon_name(self) -> &'static str {
        match self {
            Self::Appearance => "palette",
            Self::Audio => "headphones",
            Self::Equalizer => "sliders-horizontal",
            Self::Connections => "share-2",
        }
    }
}

impl NocturneApp {
    pub fn render_settings_modal(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("settings-modal-backdrop")
            .absolute()
            .inset_0()
            .bg(rgba(0x040408D8))
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.0))
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings_modal_open = false;
                cx.notify();
            }))
            .child(
                div()
                    .id("settings-modal-card")
                    .w(px(790.0))
                    .max_h(px(690.0))
                    .p(px(20.0))
                    .rounded(px(14.0))
                    .bg(rgb(pal.modal_bg))
                    .border_1()
                    .border_color(rgb(pal.border_strong))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.stop_propagation();
                    }))
                    .child(self.render_modal_header(pal, cx))
                    .child(self.render_settings_tabs(pal, cx))
                    .child(
                        div()
                            .id("settings-modal-scroll")
                            .flex()
                            .flex_col()
                            .gap(px(14.0))
                            .overflow_y_scroll()
                            .when(self.settings_tab == SettingsTab::Appearance, |body| {
                                body.child(self.render_theme_section(pal, cx))
                                    .child(self.render_language_section(pal, cx))
                            })
                            .when(self.settings_tab == SettingsTab::Audio, |body| {
                                body.child(self.render_device_section(pal, cx))
                                    .child(self.render_wasapi_section(pal, cx))
                                    .child(self.render_stream_quality_section(pal, cx))
                            })
                            .when(self.settings_tab == SettingsTab::Equalizer, |body| {
                                body.child(self.render_eq_section(pal, cx))
                            })
                            .when(self.settings_tab == SettingsTab::Connections, |body| {
                                body.child(self.render_discord_section(pal, cx))
                                    .child(self.render_lastfm_section(pal, cx))
                                    .child(self.render_update_section(pal, cx))
                            }),
                    ),
            )
    }

    fn render_settings_tabs(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .gap(px(7.0))
            .pb(px(10.0))
            .border_b_1()
            .border_color(rgb(pal.border_subtle))
            .children(
                SettingsTab::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        let selected = self.settings_tab == tab;
                        let text_col = rgb(if selected {
                            pal.accent_soft_text
                        } else {
                            pal.text_secondary
                        });
                        div()
                            .id(ElementId::Name(format!("settings-tab-{index}").into()))
                            .px(px(14.0))
                            .py(px(7.0))
                            .rounded(px(7.0))
                            .bg(rgb(if selected {
                                pal.bg_row_active
                            } else {
                                pal.bg_chip
                            }))
                            .border_1()
                            .border_color(rgb(if selected {
                                pal.accent
                            } else {
                                pal.border_subtle
                            }))
                            .hover(|style| style.bg(rgb(pal.bg_row_hover)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(12.0))
                            .font_weight(if selected {
                                gpui::FontWeight::BOLD
                            } else {
                                gpui::FontWeight::MEDIUM
                            })
                            .text_color(text_col)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings_tab = tab;
                                cx.notify();
                            }))
                            .child(lucide_icon(tab.icon_name(), 14.0, text_col))
                            .child(text(self.language, tab.key()))
                    }),
            )
    }

    fn render_modal_header(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .pb(px(10.0))
            .border_b_1()
            .border_color(rgb(pal.border_subtle))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(pal.text_primary))
                            .child(text(
                                self.language,
                                "Nocturne • Görünüm, Ses & Ekolayzer Ayarları",
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(rgb(pal.text_secondary))
                            .child(text(
                                self.language,
                                "GPUI temaları, çıkış donanımı, WASAPI, 10-bant EQ ve bağlantılar",
                            )),
                    ),
            )
            .child(
                div()
                    .id("close-settings-modal")
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(7.0))
                    .bg(rgb(pal.bg_chip))
                    .hover(|s| s.bg(rgb(pal.accent)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .text_size(px(11.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(pal.text_primary))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings_modal_open = false;
                        cx.notify();
                    }))
                    .child(lucide_icon("x", 14.0, rgb(pal.text_primary)))
                    .child("ESC"),
            )
    }

    fn render_discord_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let enabled = self.discord_rpc.is_enabled();
        let connected = self.discord_rpc.is_connected();

        let status_label = if !enabled {
            text(self.language, "Devre dışı")
        } else if connected {
            text(self.language, "Discord'a bağlı")
        } else {
            text(self.language, "Discord istemcisi bekleniyor...")
        };

        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(pal.accent_soft_text))
                    .child("Discord Rich Presence"),
            )
            .child(
                div()
                    .id("toggle-discord-rpc")
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .bg(rgb(pal.bg_row))
                    .border_1()
                    .border_color(if enabled {
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
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_discord_rpc();
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(pal.text_primary))
                                    .child(text(
                                        self.language,
                                        "Dinlediğin şarkıyı Discord profilinde göster",
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(if connected && enabled {
                                        rgb(0x6EE7B7)
                                    } else {
                                        rgb(pal.text_secondary)
                                    })
                                    .child(status_label),
                            ),
                    )
                    .child(
                        div()
                            .w(px(36.0))
                            .h(px(20.0))
                            .rounded(px(10.0))
                            .px(px(2.5))
                            .bg(if enabled {
                                rgb(pal.accent)
                            } else {
                                rgb(pal.bg_chip)
                            })
                            .flex()
                            .items_center()
                            .justify_end()
                            .when(!enabled, |sw| sw.justify_start())
                            .child(
                                div()
                                    .w(px(15.0))
                                    .h(px(15.0))
                                    .rounded(px(7.5))
                                    .bg(rgb(pal.text_primary)),
                            ),
                    ),
            )
    }

    fn render_lastfm_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let enabled = self.lastfm.is_enabled();
        let username = self.lastfm.username();
        let is_logged_in = username.is_some();
        let status_msg = self.lastfm.status_message();

        if !is_logged_in {
            self.lastfm.prefetch_auth_token();
        }

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(pal.accent_soft_text))
                    .child("Last.fm Scrobbling"),
            )
            .child(
                div()
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .bg(rgb(pal.bg_row))
                    .border_1()
                    .border_color(if is_logged_in && enabled {
                        rgb(pal.accent)
                    } else {
                        rgb(pal.border_subtle)
                    })
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(pal.text_primary))
                                            .child(if let Some(ref user) = username {
                                                format!(
                                                    "{}: @{user}",
                                                    text(self.language, "Bağlı Hesap")
                                                )
                                            } else {
                                                text(self.language, "Last.fm hesabı bağlı değil")
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.5))
                                            .text_color(rgb(pal.text_secondary))
                                            .child(text(
                                                self.language,
                                                "Parçanın %50'si veya 4 dakikası dinlenince otomatik scrobble gönderilir.",
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .when(!is_logged_in, |btns| {
                                        btns.child(
                                            div()
                                                .id("lastfm-start-auth")
                                                .px(px(11.0))
                                                .py(px(6.0))
                                                .rounded(px(6.0))
                                                .bg(rgb(pal.accent))
                                                .hover(|s| s.bg(rgb(pal.accent_bright)))
                                                .cursor_pointer()
                                                .text_size(px(11.5))
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(rgb(pal.text_primary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.lastfm.start_auth();
                                                    cx.notify();
                                                }))
                                                .child(text(self.language, "Last.fm ile Giriş Yap")),
                                        )
                                    })
                                    .when(is_logged_in, |btns| {
                                        btns.child(
                                            div()
                                                .id("lastfm-logout")
                                                .px(px(10.0))
                                                .py(px(6.0))
                                                .rounded(px(6.0))
                                                .bg(rgb(pal.bg_chip))
                                                .hover(|s| s.bg(rgb(0x7F1D1D)))
                                                .cursor_pointer()
                                                .text_size(px(11.0))
                                                .text_color(rgb(pal.text_secondary))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.lastfm.logout();
                                                    cx.notify();
                                                }))
                                                .child(text(self.language, "Bağlantıyı Kes")),
                                        )
                                    }),
                            ),
                    )
                    .when_some(status_msg, |card, msg| {
                        card.child(
                            div()
                                .px(px(9.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .bg(rgb(pal.bg_root))
                                .text_size(px(10.5))
                                .text_color(rgb(pal.accent_bright))
                                .child(msg),
                        )
                    })
                    .child(
                        div()
                            .id("toggle-lastfm-scrobble")
                            .pt(px(8.0))
                            .border_t_1()
                            .border_color(rgb(pal.border_subtle))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_between()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_lastfm_enabled();
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgb(pal.text_primary))
                                    .child(text(self.language, "Scrobbling & Now Playing Aktif")),
                            )
                            .child(
                                div()
                                    .w(px(36.0))
                                    .h(px(20.0))
                                    .rounded(px(10.0))
                                    .px(px(2.5))
                                    .bg(if enabled {
                                        rgb(pal.accent)
                                    } else {
                                        rgb(pal.bg_chip)
                                    })
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .when(!enabled, |sw| sw.justify_start())
                                    .child(
                                        div()
                                            .w(px(15.0))
                                            .h(px(15.0))
                                            .rounded(px(7.5))
                                            .bg(rgb(pal.text_primary)),
                                    ),
                            ),
                    ),
            )
    }

    fn render_update_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let current_ver = self.updater.current_version();
        let state = self.updater.state();

        let (status_text, status_color, available_ver, is_checking) = match state {
            UpdateCheckState::Idle => (
                format!("{}: v{current_ver}", text(self.language, "Mevcut sürüm")),
                rgb(pal.text_secondary),
                None,
                false,
            ),
            UpdateCheckState::Checking => (
                text(self.language, "GitHub sürümleri kontrol ediliyor..."),
                rgb(pal.accent_bright),
                None,
                true,
            ),
            UpdateCheckState::UpToDate => (
                format!("{} (v{current_ver})", text(self.language, "Güncelsiniz")),
                rgb(0x6EE7B7),
                None,
                false,
            ),
            UpdateCheckState::Available(info) => (
                format!(
                    "{}: v{} ({}: v{current_ver})",
                    text(self.language, "Yeni sürüm hazır"),
                    info.latest_version,
                    text(self.language, "Mevcut")
                ),
                rgb(0x6EE7B7),
                Some(info.latest_version),
                false,
            ),
            UpdateCheckState::Error(err) => (err, rgb(0xFCA5A5), None, false),
        };

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(pal.accent_soft_text))
                    .child(text(self.language, "Uygulama Güncellemeleri (GitHub)")),
            )
            .child(
                div()
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .bg(rgb(pal.bg_row))
                    .border_1()
                    .border_color(if available_ver.is_some() {
                        rgb(0x10B981)
                    } else {
                        rgb(pal.border_subtle)
                    })
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(pal.text_primary))
                                    .child(format!("Nocturne v{current_ver}")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(status_color)
                                    .child(status_text),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .when_some(available_ver, |btns, ver| {
                                btns.child(
                                    div()
                                        .id("settings-download-update")
                                        .px(px(11.0))
                                        .py(px(6.0))
                                        .rounded(px(6.0))
                                        .bg(rgb(0x166534))
                                        .hover(|s| s.bg(rgb(0x15803D)))
                                        .cursor_pointer()
                                        .flex()
                                        .items_center()
                                        .gap(px(5.0))
                                        .text_size(px(11.5))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xECFDF5))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.updater.open_update_download();
                                            cx.notify();
                                        }))
                                        .child(lucide_icon("download", 13.0, rgb(0xECFDF5)))
                                        .child(format!("v{ver} {}", text(self.language, "İndir"))),
                                )
                            })
                            .child(
                                div()
                                    .id("settings-check-updates")
                                    .px(px(11.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(pal.bg_chip))
                                    .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(11.5))
                                    .text_color(rgb(pal.text_primary))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if !is_checking {
                                            this.updater.check_now();
                                        }
                                        cx.notify();
                                    }))
                                    .child(lucide_icon("refresh-cw", 12.5, rgb(pal.accent_bright)))
                                    .child(text(self.language, "Güncellemeleri Denetle")),
                            ),
                    ),
            )
    }

    fn render_theme_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(pal.accent_soft_text))
                    .child(text(self.language, "Görünüm Teması")),
            )
            .child(div().flex().flex_wrap().gap(px(8.0)).children(
                ThemeId::ALL.iter().enumerate().map(|(idx, &t_id)| {
                    let t_pal = t_id.palette();
                    let is_active = self.theme_id == t_id;
                    div()
                        .id(ElementId::Name(format!("theme-opt-{idx}").into()))
                        .px(px(12.0))
                        .py(px(7.0))
                        .rounded(px(8.0))
                        .bg(rgb(t_pal.bg_panel))
                        .border_1()
                        .border_color(if is_active {
                            rgb(t_pal.accent_bright)
                        } else {
                            rgb(t_pal.border_strong)
                        })
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.theme_id = t_id;
                            this.save_config();
                            cx.notify();
                        }))
                        .child(
                            div()
                                .w(px(11.0))
                                .h(px(11.0))
                                .rounded(px(5.5))
                                .bg(rgb(t_pal.accent_bright)),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(if is_active {
                                    gpui::FontWeight::BOLD
                                } else {
                                    gpui::FontWeight::NORMAL
                                })
                                .text_color(rgb(t_pal.text_primary))
                                .child(t_id.full_name()),
                        )
                }),
            ))
    }

    fn render_language_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(pal.accent_soft_text))
                    .child(text(self.language, "Dil")),
            )
            .child(
                div().flex().flex_wrap().gap(px(7.0)).children(
                    LanguageSetting::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(index, language)| {
                            let selected = self.language == language;
                            let label = if language == LanguageSetting::System {
                                text(self.language, "Sistem (Otomatik)")
                            } else {
                                language.label().to_string()
                            };
                            div()
                                .id(ElementId::Name(format!("language-option-{index}").into()))
                                .px(px(10.0))
                                .py(px(6.0))
                                .rounded(px(7.0))
                                .bg(rgb(if selected {
                                    pal.bg_row_active
                                } else {
                                    pal.bg_chip
                                }))
                                .border_1()
                                .border_color(rgb(if selected {
                                    pal.accent
                                } else {
                                    pal.border_subtle
                                }))
                                .hover(|style| style.bg(rgb(pal.bg_row_hover)))
                                .cursor_pointer()
                                .text_size(px(11.5))
                                .text_color(rgb(if selected {
                                    pal.accent_soft_text
                                } else {
                                    pal.text_secondary
                                }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.language = language;
                                    this.save_config();
                                    cx.notify();
                                }))
                                .child(label)
                        }),
                ),
            )
    }

    fn render_device_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_id_clone = self.selected_device_id.clone();
        let is_sys_def = selected_id_clone.is_none();

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.5))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(pal.accent_soft_text))
                            .child(text(
                                self.language,
                                "1. Ses Çıkış Aygıtı (DAC / Kulaklık / Hoparlör)",
                            )),
                    )
                    .child(
                        div()
                            .id("refresh-audio-devices")
                            .px(px(8.0))
                            .py(px(3.5))
                            .rounded(px(5.0))
                            .bg(rgb(pal.bg_chip))
                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .text_size(px(10.5))
                            .text_color(rgb(pal.accent_bright))
                            .on_click(cx.listener(|this, _, _, cx| {
                                Self::spawn_device_scan(this.bg_tx.clone());
                                cx.notify();
                            }))
                            .child(lucide_icon("refresh-cw", 12.0, rgb(pal.accent_bright)))
                            .child(text(self.language, "Aygıtları Yenile")),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(pal.text_secondary))
                    .child(text(
                        self.language,
                        "Aygıt seçimi Exclusive çıkışta kullanılır; Shared mod Windows'un varsayılan aygıtını izler.",
                    )),
            )
            .child(
                div()
                    .id("dev-system-default")
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(8.0))
                    .bg(if is_sys_def {
                        rgb(pal.bg_row_active)
                    } else {
                        rgb(pal.bg_row)
                    })
                    .border_1()
                    .border_color(if is_sys_def {
                        rgb(pal.accent)
                    } else {
                        rgb(pal.border_subtle)
                    })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.selected_device_id = None;
                        this.audio.set_output_device(None);
                        this.save_config();
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(lucide_icon("speaker", 14.0, rgb(pal.accent_bright)))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(text(
                                        self.language,
                                        "Windows Varsayılan Çıkış Aygıtı (Otomatik)",
                                    )),
                            ),
                    )
                    .when(is_sys_def, |row| {
                        row.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(0xA7F3D0))
                                .child(text(self.language, "Seçili")),
                        )
                    }),
            )
            .children(self.output_devices.iter().enumerate().map(|(idx, dev)| {
                let dev_id = dev.id.clone();
                let is_sel = selected_id_clone.as_deref() == Some(dev.id.as_str());
                let label = if dev.is_default {
                    format!("{} ({})", dev.name, text(self.language, "Sistem Varsayılanı"))
                } else {
                    dev.name.clone()
                };
                div()
                    .id(ElementId::Name(format!("out-dev-{idx}").into()))
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(8.0))
                    .bg(if is_sel {
                        rgb(pal.bg_row_active)
                    } else {
                        rgb(pal.bg_row)
                    })
                    .border_1()
                    .border_color(if is_sel {
                        rgb(pal.accent)
                    } else {
                        rgb(pal.border_subtle)
                    })
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_between()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let next_id = Some(dev_id.clone());
                        this.selected_device_id = next_id.clone();
                        this.audio.set_output_device(next_id);
                        this.save_config();
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(lucide_icon("headphones", 14.0, rgb(pal.text_secondary)))
                            .child(div().text_size(px(12.0)).child(label)),
                    )
                    .when(is_sel, |row| {
                        row.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(0xA7F3D0))
                                .child(text(self.language, "Seçili")),
                        )
                    })
            }))
    }

    fn render_wasapi_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let diag = self.audio_diag.clone();
        let is_exclusive = self.share_mode == OutputShareMode::Exclusive;
        let eq_status = if !self.eq_enabled {
            text(self.language, "Kapalı")
        } else if diag.eq_active {
            text(
                self.language,
                "İlk parçadan itibaren ses akışına uygulanıyor",
            )
        } else {
            text(self.language, "Açık, WebView2 ses zincirine bağlanıyor")
        };

        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(pal.accent_soft_text))
                    .child(text(
                        self.language,
                        "2. WASAPI Çıkış Modu & Donanım Doğrulama",
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("modal-wasapi-shared")
                            .flex_1()
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(if !is_exclusive {
                                rgb(pal.bg_row_active)
                            } else {
                                rgb(pal.bg_row)
                            })
                            .border_1()
                            .border_color(if !is_exclusive {
                                rgb(pal.accent)
                            } else {
                                rgb(pal.border_subtle)
                            })
                            .cursor_pointer()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.share_mode = OutputShareMode::Shared;
                                this.audio.set_mode(OutputShareMode::Shared);
                                this.save_config();
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(text(self.language, "WASAPI Shared")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(pal.text_secondary))
                                    .child(text(
                                        self.language,
                                        "Diğer uygulamalarla birlikte Windows'un varsayılan çıkışını kullanır.",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("modal-wasapi-exclusive")
                            .flex_1()
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(if is_exclusive {
                                rgb(pal.bg_row_active)
                            } else {
                                rgb(pal.bg_row)
                            })
                            .border_1()
                            .border_color(if is_exclusive {
                                rgb(0x10B981)
                            } else {
                                rgb(pal.border_subtle)
                            })
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(text(self.language, "WASAPI Exclusive (Özel çıkış)")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(pal.text_secondary))
                                    .child(text(
                                        self.language,
                                        "WebView2 ses kaynağı bu modu desteklemediği için şu an kullanılamıyor.",
                                    )),
                            ),
                    ),
            )
            .child(
                div()
                    .p(px(9.0))
                    .rounded(px(8.0))
                    .bg(rgb(pal.bg_root))
                    .border_1()
                    .border_color(if diag.exclusive_verified {
                        rgb(0x10B981)
                    } else {
                        rgb(pal.border_subtle)
                    })
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(if diag.exclusive_verified {
                                rgb(0x6EE7B7)
                            } else {
                                rgb(pal.accent_bright)
                            })
                            .child(format!(
                                "{}: {} • {} Hz / {}-bit / {} {}",
                                text(self.language, "Aktif Donanım"),
                                if diag.active_device_name == "Varsayılan Çıkış Aygıtı" {
                                    text(self.language, "Windows Varsayılan Çıkış Aygıtı (Otomatik)")
                                } else {
                                    diag.active_device_name
                                },
                                diag.sample_rate,
                                diag.bit_depth,
                                diag.channels,
                                text(self.language, "Kanal")
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(rgb(pal.text_secondary))
                            .child(diag.status_detail),
                    )
                    .child(
                        div()
                            .text_size(px(10.5))
                            .text_color(if diag.eq_active {
                                rgb(0x6EE7B7)
                            } else {
                                rgb(pal.text_secondary)
                            })
                            .child(format!("{}: {eq_status}", text(self.language, "Ekolayzer"))),
                    ),
            )
    }

    fn render_stream_quality_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let current_quality = self.audio_quality;
        let normalize = self.normalize_volume;

        div()
            .flex()
            .flex_col()
            .gap(px(7.0))
            .child(
                div()
                    .text_size(px(12.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(pal.accent_soft_text))
                    .child(text(
                        self.language,
                        "3. Ses Kalitesi & Akış Çözünürlüğü (Deezer)",
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .children(AudioQuality::ALL.into_iter().enumerate().map(|(idx, q)| {
                        let is_selected = current_quality == q;
                        div()
                            .id(ElementId::Name(format!("stream-quality-{idx}").into()))
                            .flex_1()
                            .p(px(10.0))
                            .rounded(px(8.0))
                            .bg(if is_selected {
                                rgb(pal.bg_row_active)
                            } else {
                                rgb(pal.bg_row)
                            })
                            .border_1()
                            .border_color(if is_selected {
                                rgb(pal.accent)
                            } else {
                                rgb(pal.border_subtle)
                            })
                            .hover(|s| s.bg(rgb(pal.bg_row_hover)))
                            .cursor_pointer()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.audio_quality = q;
                                this.engine
                                    .set_audio_quality(this.audio_quality, this.normalize_volume);
                                this.save_config();
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(7.0))
                                            .child(
                                                div()
                                                    .w(px(12.0))
                                                    .h(px(12.0))
                                                    .rounded(px(6.0))
                                                    .border_1()
                                                    .border_color(if is_selected {
                                                        rgb(pal.accent_bright)
                                                    } else {
                                                        rgb(pal.text_muted)
                                                    })
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .when(is_selected, |dot| {
                                                        dot.child(
                                                            div()
                                                                .w(px(6.0))
                                                                .h(px(6.0))
                                                                .rounded(px(3.0))
                                                                .bg(rgb(pal.accent_bright)),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(if is_selected {
                                                        rgb(pal.accent_soft_text)
                                                    } else {
                                                        rgb(pal.text_primary)
                                                    })
                                                    .child(text(self.language, q.title())),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .pl(px(19.0))
                                    .text_size(px(10.5))
                                    .text_color(if is_selected {
                                        rgb(pal.accent_bright)
                                    } else {
                                        rgb(pal.text_secondary)
                                    })
                                    .child(text(self.language, q.subtitle())),
                            )
                    })),
            )
            .child(
                div()
                    .id("toggle-volume-normalize")
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(8.0))
                    .bg(rgb(pal.bg_row))
                    .border_1()
                    .border_color(if normalize {
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
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.normalize_volume = !this.normalize_volume;
                        this.engine
                            .set_audio_quality(this.audio_quality, this.normalize_volume);
                        this.save_config();
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(pal.text_primary))
                                    .child(text(self.language, "Sesi dengele")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(pal.text_secondary))
                                    .child(text(
                                        self.language,
                                        "Ses düzeyini aynı seviyeyi koruyacak şekilde ayarlar (Hi-Fi için kapalı önerilir).",
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .w(px(36.0))
                            .h(px(20.0))
                            .rounded(px(10.0))
                            .px(px(2.5))
                            .bg(if normalize {
                                rgb(pal.accent)
                            } else {
                                rgb(pal.bg_chip)
                            })
                            .flex()
                            .items_center()
                            .justify_end()
                            .when(!normalize, |sw| sw.justify_start())
                            .child(
                                div()
                                    .w(px(15.0))
                                    .h(px(15.0))
                                    .rounded(px(7.5))
                                    .bg(rgb(pal.text_primary)),
                            ),
                    ),
            )
    }

    fn render_eq_section(&mut self, pal: ThemePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let bands = self.eq_bands;
        let eq_enabled = self.eq_enabled;

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(pal.accent_soft_text))
                                    .child(text(
                                        self.language,
                                        "3. 10-Bant Grafik Ekolayzer (BiQuad)",
                                    )),
                            )
                            .child(
                                div()
                                    .id("modal-eq-bypass")
                                    .px(px(9.0))
                                    .py(px(3.0))
                                    .rounded(px(5.0))
                                    .bg(if eq_enabled {
                                        rgb(0x166534)
                                    } else {
                                        rgb(pal.bg_chip)
                                    })
                                    .cursor_pointer()
                                    .text_size(px(10.5))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.eq_enabled = !this.eq_enabled;
                                        this.sync_eq();
                                        cx.notify();
                                    }))
                                    .child(text(
                                        self.language,
                                        if eq_enabled {
                                            "EQ Aktif"
                                        } else {
                                            "Kapalı (Bypass)"
                                        },
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(Self::preset_chip("flat-btn", "Flat", [0.0; 10], pal, cx))
                            .child(Self::preset_chip(
                                "harman-oe-btn",
                                "Harman Over-Ear",
                                [3.2, 2.5, 0.8, -0.5, 0.0, 0.5, 1.5, 2.2, 1.8, 1.0],
                                pal,
                                cx,
                            ))
                            .child(Self::preset_chip(
                                "harman-iem-btn",
                                "Harman IEM",
                                [4.0, 3.0, 1.0, -1.0, 0.0, 1.2, 2.5, 2.0, 1.0, 0.5],
                                pal,
                                cx,
                            ))
                            .child(Self::preset_chip(
                                "bass-boost-btn",
                                "Phonk / Bass",
                                [6.0, 4.5, 2.0, 0.0, -0.5, 0.0, 1.0, 2.0, 2.5, 2.0],
                                pal,
                                cx,
                            )),
                    ),
            )
            .child(self.render_preamp_row(pal, cx))
            .child(self.render_presets_section(pal, cx))
            .child({
                let mut band_views = Vec::with_capacity(10);
                for (idx, gain) in bands.iter().copied().enumerate() {
                    band_views.push(Self::render_eq_band(idx, gain, eq_enabled, pal, cx));
                }
                div()
                    .flex()
                    .justify_between()
                    .gap(px(6.0))
                    .children(band_views)
            })
    }

    fn render_eq_band(
        idx: usize,
        gain: f32,
        eq_enabled: bool,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let norm = ((gain + 12.0) / 24.0).clamp(0.0, 1.0);

        div()
            .flex_1()
            .py(px(6.0))
            .px(px(4.0))
            .rounded(px(8.0))
            .bg(rgb(pal.bg_row))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(pal.accent_bright))
                    .child(format!("{:+.1}", gain)),
            )
            .child(
                div()
                    .w(px(6.0))
                    .h(px(52.0))
                    .rounded(px(3.0))
                    .bg(rgb(pal.bg_chip))
                    .flex()
                    .items_end()
                    .child(
                        div()
                            .w_full()
                            .h(px(norm * 52.0))
                            .rounded(px(3.0))
                            .bg(if eq_enabled {
                                rgb(pal.accent_bright)
                            } else {
                                rgb(pal.text_muted)
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id(ElementId::Name(format!("eq-down-{idx}").into()))
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded(px(4.0))
                            .bg(rgb(pal.bg_chip))
                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.eq_enabled = true;
                                this.eq_bands[idx] = (this.eq_bands[idx] - 1.0).clamp(-12.0, 12.0);
                                this.eq_preset_name = "Custom".into();
                                this.sync_eq();
                                cx.notify();
                            }))
                            .child(lucide_icon("minus", 11.0, rgb(pal.text_primary))),
                    )
                    .child(
                        div()
                            .id(ElementId::Name(format!("eq-up-{idx}").into()))
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded(px(4.0))
                            .bg(rgb(pal.bg_chip))
                            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.eq_enabled = true;
                                this.eq_bands[idx] = (this.eq_bands[idx] + 1.0).clamp(-12.0, 12.0);
                                this.eq_preset_name = "Custom".into();
                                this.sync_eq();
                                cx.notify();
                            }))
                            .child(lucide_icon("plus", 11.0, rgb(pal.text_primary))),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(pal.text_secondary))
                    .child(EQ_FREQS[idx]),
            )
    }

    fn render_preamp_row(&mut self, pal: ThemePalette, cx: &mut Context<Self>) -> impl IntoElement {
        let preamp = self.eq_preamp_db;
        let preamp_text = format!("{preamp:+.1} dB");

        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.0))
            .py(px(7.0))
            .rounded(px(8.0))
            .bg(rgb(pal.bg_row))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(pal.text_primary))
                            .child(text(self.language, "Ön Yükselteç (Preamp)")),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(pal.text_secondary))
                            .child(text(self.language, "-12.0 dB … +6.0 dB • 0.5 dB adım")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.preamp_button("eq-preamp-down", "minus", -1.0, pal, cx))
                    .child(
                        div()
                            .w(px(68.0))
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(if self.eq_enabled {
                                rgb(pal.accent_bright)
                            } else {
                                rgb(pal.text_muted)
                            })
                            .child(preamp_text),
                    )
                    .child(self.preamp_button("eq-preamp-up", "plus", 1.0, pal, cx)),
            )
    }

    fn preamp_button(
        &mut self,
        id: &'static str,
        icon_name: &'static str,
        direction: f32,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .w(px(26.0))
            .h(px(22.0))
            .rounded(px(5.0))
            .bg(rgb(pal.bg_chip))
            .hover(|s| s.bg(rgb(pal.bg_chip_hover)))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.eq_enabled = true;
                this.eq_preamp_db =
                    crate::clamp_preamp_db(this.eq_preamp_db + direction * crate::EQ_PREAMP_STEP);
                this.eq_preset_name = "Custom".into();
                this.sync_eq();
                cx.notify();
            }))
            .child(lucide_icon(icon_name, 13.0, rgb(pal.text_primary)))
    }

    fn render_presets_section(
        &mut self,
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let presets = self.eq_presets.clone();
        let selected_name = self.eq_preset_name.to_string();
        let input_active = self.preset_name_focused;
        let input_text = self.eq_preset_input.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(rgb(pal.accent_soft_text))
                            .child(text(self.language, "Özel EQ Presetleri")),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(pal.text_secondary))
                            .child(format!("presets.json • {}", presets.len())),
                    ),
            )
            .when(presets.is_empty(), |section| {
                section.child(
                    div()
                        .px(px(8.0))
                        .text_size(px(10.5))
                        .text_color(rgb(pal.text_muted))
                        .child(text(self.language, "Henüz kayıtlı preset yok.")),
                )
            })
            .when(!presets.is_empty(), |section| {
                section.child(div().flex().flex_wrap().gap(px(6.0)).children(
                    presets.into_iter().enumerate().map(|(idx, preset)| {
                        let is_selected = selected_name == preset.name;
                        let preset_name = preset.name.clone();
                        div()
                            .id(ElementId::Name(format!("eq-preset-{idx}").into()))
                            .flex()
                            .items_center()
                            .rounded(px(6.0))
                            .bg(rgb(if is_selected {
                                pal.bg_row_active
                            } else {
                                pal.bg_chip
                            }))
                            .border_1()
                            .border_color(if is_selected {
                                rgb(pal.accent)
                            } else {
                                rgb(pal.border_subtle)
                            })
                            .child(
                                div()
                                    .id(ElementId::Name(format!("eq-preset-apply-{idx}").into()))
                                    .px(px(9.0))
                                    .py(px(4.0))
                                    .cursor_pointer()
                                    .text_size(px(11.0))
                                    .text_color(rgb(if is_selected {
                                        pal.accent_soft_text
                                    } else {
                                        pal.text_primary
                                    }))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.apply_saved_preset(idx);
                                        cx.notify();
                                    }))
                                    .child(preset_name),
                            )
                            .child(
                                div()
                                    .id(ElementId::Name(format!("eq-preset-delete-{idx}").into()))
                                    .px(px(7.0))
                                    .py(px(4.0))
                                    .rounded(px(5.0))
                                    .hover(|s| s.bg(rgb(0x7F1D1D)))
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_saved_preset(idx);
                                        cx.notify();
                                    }))
                                    .child(lucide_icon("trash-2", 11.5, rgb(pal.text_secondary))),
                            )
                    }),
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        div()
                            .id("eq-preset-name-input")
                            .flex_1()
                            .h(px(28.0))
                            .px(px(9.0))
                            .rounded(px(6.0))
                            .bg(rgb(pal.bg_root))
                            .border_1()
                            .border_color(if input_active {
                                rgb(pal.accent)
                            } else {
                                rgb(pal.border_subtle)
                            })
                            .flex()
                            .items_center()
                            .cursor_text()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.preset_name_focused = true;
                                window.focus(&this.focus_handle, cx);
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(if input_text.is_empty() {
                                        rgb(pal.text_muted)
                                    } else {
                                        rgb(pal.text_primary)
                                    })
                                    .child(if input_text.is_empty() {
                                        text(self.language, "Preset adı yaz, Enter ile kaydet")
                                    } else {
                                        format!("{input_text}|")
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("eq-preset-save")
                            .px(px(10.0))
                            .py(px(5.0))
                            .rounded(px(6.0))
                            .bg(rgb(pal.accent))
                            .hover(|s| s.bg(rgb(pal.accent_bright)))
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_preset_from_input();
                                cx.notify();
                            }))
                            .child(text(self.language, "Preset Kaydet")),
                    ),
            )
    }

    fn preset_chip(
        id: &'static str,
        label: &'static str,
        bands: [f32; 10],
        pal: ThemePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(5.0))
            .bg(rgb(pal.bg_chip))
            .hover(|s| s.bg(rgb(pal.accent)))
            .cursor_pointer()
            .text_size(px(11.0))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.apply_preset(label, bands);
                cx.notify();
            }))
            .child(label)
    }
}
