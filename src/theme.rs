//! Device-local appearance. `color_scheme` and `color_mode` live in `rurarendar.ron`.
//! They are not sync items.

use crate::ui::{
    AppWindow,
    Theme,
};
use slint::ComponentHandle;

/// Preset named in Settings. Index matches `Theme.scheme-index`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorSchemeId {
    Eink,
    Gruvbox,
    #[default]
    Modern,
}

/// Light/dark choice. Index matches `Theme.mode-index`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorMode {
    #[default]
    Auto,
    Light,
    Dark,
}

impl ColorSchemeId {
    pub fn as_str(self) -> &'static str {
        match self {
            ColorSchemeId::Eink => "eink",
            ColorSchemeId::Gruvbox => "gruvbox",
            ColorSchemeId::Modern => "modern",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "eink" => ColorSchemeId::Eink,
            "gruvbox" => ColorSchemeId::Gruvbox,
            _ => ColorSchemeId::Modern,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            1 => ColorSchemeId::Gruvbox,
            2 => ColorSchemeId::Modern,
            _ => ColorSchemeId::Eink,
        }
    }

    pub fn index(self) -> i32 {
        match self {
            ColorSchemeId::Eink => 0,
            ColorSchemeId::Gruvbox => 1,
            ColorSchemeId::Modern => 2,
        }
    }
}

impl ColorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ColorMode::Auto => "auto",
            ColorMode::Light => "light",
            ColorMode::Dark => "dark",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => ColorMode::Light,
            "dark" => ColorMode::Dark,
            _ => ColorMode::Auto,
        }
    }

    pub fn from_index(index: i32) -> Self {
        match index {
            1 => ColorMode::Light,
            2 => ColorMode::Dark,
            _ => ColorMode::Auto,
        }
    }

    pub fn index(self) -> i32 {
        match self {
            ColorMode::Auto => 0,
            ColorMode::Light => 1,
            ColorMode::Dark => 2,
        }
    }
}

pub fn apply(ui: &AppWindow, scheme: ColorSchemeId, mode: ColorMode) {
    let theme = ui.global::<Theme>();
    theme.set_scheme_index(scheme.index());
    theme.set_mode_index(mode.index());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_roundtrip() {
        for scheme in [ColorSchemeId::Eink, ColorSchemeId::Gruvbox, ColorSchemeId::Modern] {
            assert_eq!(ColorSchemeId::from_setting(scheme.as_str()), scheme);
            assert_eq!(ColorSchemeId::from_index(scheme.index()), scheme);
        }
        for mode in [ColorMode::Auto, ColorMode::Light, ColorMode::Dark] {
            assert_eq!(ColorMode::from_setting(mode.as_str()), mode);
            assert_eq!(ColorMode::from_index(mode.index()), mode);
        }
        assert_eq!(ColorSchemeId::default(), ColorSchemeId::Modern);
        assert_eq!(ColorSchemeId::from_setting(" GRUVBOX "), ColorSchemeId::Gruvbox);
        assert_eq!(ColorSchemeId::from_setting("eink"), ColorSchemeId::Eink);
        assert_eq!(ColorSchemeId::from_setting("nope"), ColorSchemeId::Modern);
        assert_eq!(ColorSchemeId::from_setting(""), ColorSchemeId::Modern);
        assert_eq!(ColorMode::from_setting(""), ColorMode::Auto);
        assert_eq!(ColorMode::from_setting("DARK"), ColorMode::Dark);
    }
}
