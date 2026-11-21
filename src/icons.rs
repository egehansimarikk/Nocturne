use gpui::{svg, AssetSource, Result, Rgba, SharedString, Styled, Svg};
use std::borrow::Cow;

pub struct NocturneAssets;

macro_rules! embed_lucide {
    ($path:expr, [ $( $file:literal ),* $(,)? ]) => {
        match $path {
            $(
                concat!("icons/lucide/", $file) => Some(Cow::Borrowed(include_bytes!(concat!(
                    "../assets/icons/lucide/",
                    $file
                )))),
            )*
            _ => None,
        }
    };
}

impl AssetSource for NocturneAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(embed_lucide!(
            path,
            [
                "chevron-left.svg",
                "chevron-right.svg",
                "circle-user-round.svg",
                "disc-3.svg",
                "headphones.svg",
                "heart-filled.svg",
                "heart.svg",
                "list-music.svg",
                "list-plus.svg",
                "log-out.svg",
                "minus.svg",
                "music.svg",
                "palette.svg",
                "pause-filled.svg",
                "play-filled.svg",
                "play.svg",
                "plus.svg",
                "refresh-cw.svg",
                "repeat-one.svg",
                "repeat.svg",
                "search.svg",
                "settings.svg",
                "share-2.svg",
                "shuffle.svg",
                "skip-back.svg",
                "skip-forward.svg",
                "sliders-horizontal.svg",
                "speaker.svg",
                "trash-2.svg",
                "volume-1.svg",
                "volume-2.svg",
                "volume-x.svg",
                "x.svg",
            ]
        ))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let all = [
            "icons/lucide/chevron-left.svg",
            "icons/lucide/chevron-right.svg",
            "icons/lucide/circle-user-round.svg",
            "icons/lucide/disc-3.svg",
            "icons/lucide/headphones.svg",
            "icons/lucide/heart-filled.svg",
            "icons/lucide/heart.svg",
            "icons/lucide/list-music.svg",
            "icons/lucide/list-plus.svg",
            "icons/lucide/log-out.svg",
            "icons/lucide/minus.svg",
            "icons/lucide/music.svg",
            "icons/lucide/palette.svg",
            "icons/lucide/pause-filled.svg",
            "icons/lucide/play-filled.svg",
            "icons/lucide/play.svg",
            "icons/lucide/plus.svg",
            "icons/lucide/refresh-cw.svg",
            "icons/lucide/repeat-one.svg",
            "icons/lucide/repeat.svg",
            "icons/lucide/search.svg",
            "icons/lucide/settings.svg",
            "icons/lucide/share-2.svg",
            "icons/lucide/shuffle.svg",
            "icons/lucide/skip-back.svg",
            "icons/lucide/skip-forward.svg",
            "icons/lucide/sliders-horizontal.svg",
            "icons/lucide/speaker.svg",
            "icons/lucide/trash-2.svg",
            "icons/lucide/volume-1.svg",
            "icons/lucide/volume-2.svg",
            "icons/lucide/volume-x.svg",
            "icons/lucide/x.svg",
        ];

        Ok(all
            .iter()
            .filter(|entry| entry.starts_with(path))
            .map(|&entry| SharedString::from(entry))
            .collect())
    }
}

pub fn lucide_icon(name: &'static str, size_px: f32, color: Rgba) -> Svg {
    svg()
        .path(format!("icons/lucide/{name}.svg"))
        .size(gpui::px(size_px))
        .text_color(color)
        .flex_shrink_0()
}