use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeId {
    #[default]
    Obsidian,
    Amoled,
    Nebula,
    WarmVinyl,
    NordicSlate,
}

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub bg_root: u32,
    pub bg_topbar: u32,
    pub bg_sidebar: u32,
    pub bg_panel: u32,
    pub bg_row: u32,
    pub bg_row_hover: u32,
    pub bg_row_active: u32,
    pub bg_chip: u32,
    pub bg_chip_hover: u32,
    pub border_subtle: u32,
    pub border_strong: u32,
    pub accent: u32,
    pub accent_bright: u32,
    pub accent_soft_text: u32,
    pub text_primary: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub modal_bg: u32,
}

impl ThemeId {
    pub const ALL: [ThemeId; 5] = [
        ThemeId::Obsidian,
        ThemeId::Amoled,
        ThemeId::Nebula,
        ThemeId::WarmVinyl,
        ThemeId::NordicSlate,
    ];

    pub fn next(self) -> Self {
        match self {
            ThemeId::Obsidian => ThemeId::Amoled,
            ThemeId::Amoled => ThemeId::Nebula,
            ThemeId::Nebula => ThemeId::WarmVinyl,
            ThemeId::WarmVinyl => ThemeId::NordicSlate,
            ThemeId::NordicSlate => ThemeId::Obsidian,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Obsidian => "Obsidian",
            ThemeId::Amoled => "AMOLED",
            ThemeId::Nebula => "Nebula",
            ThemeId::WarmVinyl => "Vinyl",
            ThemeId::NordicSlate => "Nordic",
        }
    }

    pub fn full_name(self) -> &'static str {
        match self {
            ThemeId::Obsidian => "Nocturne Obsidian",
            ThemeId::Amoled => "AMOLED Pure Black",
            ThemeId::Nebula => "Midnight Nebula",
            ThemeId::WarmVinyl => "Studio Warm Vinyl",
            ThemeId::NordicSlate => "Nordic Slate",
        }
    }

    pub fn palette(self) -> ThemePalette {
        match self {
            ThemeId::Obsidian => ThemePalette {
                bg_root: 0x09090E,
                bg_topbar: 0x0F0F17,
                bg_sidebar: 0x0C0C13,
                bg_panel: 0x12121B,
                bg_row: 0x13131E,
                bg_row_hover: 0x1C1C2B,
                bg_row_active: 0x241738,
                bg_chip: 0x1C1C2A,
                bg_chip_hover: 0x2A2A3E,
                border_subtle: 0x1F1F2E,
                border_strong: 0x313148,
                accent: 0x9333EA,
                accent_bright: 0xC084FC,
                accent_soft_text: 0xE9D5FF,
                text_primary: 0xF4F4F8,
                text_secondary: 0xA1A1B8,
                text_muted: 0x6E6E87,
                modal_bg: 0x11111B,
            },
            ThemeId::Amoled => ThemePalette {
                bg_root: 0x000000,
                bg_topbar: 0x050507,
                bg_sidebar: 0x030305,
                bg_panel: 0x08080B,
                bg_row: 0x0B0B0F,
                bg_row_hover: 0x15151C,
                bg_row_active: 0x1F1330,
                bg_chip: 0x121218,
                bg_chip_hover: 0x20202A,
                border_subtle: 0x1A1A22,
                border_strong: 0x2C2C38,
                accent: 0xA855F7,
                accent_bright: 0xD8B4FE,
                accent_soft_text: 0xF3E8FF,
                text_primary: 0xFFFFFF,
                text_secondary: 0xA3A3B2,
                text_muted: 0x666675,
                modal_bg: 0x09090D,
            },
            ThemeId::Nebula => ThemePalette {
                bg_root: 0x0A0B1A,
                bg_topbar: 0x0F1126,
                bg_sidebar: 0x0C0E20,
                bg_panel: 0x131630,
                bg_row: 0x141833,
                bg_row_hover: 0x1D2247,
                bg_row_active: 0x281D52,
                bg_chip: 0x1B2042,
                bg_chip_hover: 0x28305E,
                border_subtle: 0x22274D,
                border_strong: 0x363E73,
                accent: 0x7C3AED,
                accent_bright: 0xA78BFA,
                accent_soft_text: 0xEDE9FE,
                text_primary: 0xF1F5F9,
                text_secondary: 0x94A3B8,
                text_muted: 0x64748B,
                modal_bg: 0x10132B,
            },
            ThemeId::WarmVinyl => ThemePalette {
                bg_root: 0x0F0C0A,
                bg_topbar: 0x16120F,
                bg_sidebar: 0x130F0C,
                bg_panel: 0x1A1511,
                bg_row: 0x1C1612,
                bg_row_hover: 0x261E18,
                bg_row_active: 0x332214,
                bg_chip: 0x241D17,
                bg_chip_hover: 0x332920,
                border_subtle: 0x2B221B,
                border_strong: 0x423429,
                accent: 0xD97706,
                accent_bright: 0xFBBF24,
                accent_soft_text: 0xFEF3C7,
                text_primary: 0xFAF5F0,
                text_secondary: 0xB8ABA0,
                text_muted: 0x7D7066,
                modal_bg: 0x17120E,
            },
            ThemeId::NordicSlate => ThemePalette {
                bg_root: 0x0B111A,
                bg_topbar: 0x101824,
                bg_sidebar: 0x0D141F,
                bg_panel: 0x131D2B,
                bg_row: 0x152030,
                bg_row_hover: 0x1D2C42,
                bg_row_active: 0x18344F,
                bg_chip: 0x1B293D,
                bg_chip_hover: 0x263954,
                border_subtle: 0x203047,
                border_strong: 0x31496B,
                accent: 0x0284C7,
                accent_bright: 0x38BDF8,
                accent_soft_text: 0xE0F2FE,
                text_primary: 0xF1F5F9,
                text_secondary: 0x94A3B8,
                text_muted: 0x64748B,
                modal_bg: 0x101824,
            },
        }
    }
}