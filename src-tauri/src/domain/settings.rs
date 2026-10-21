use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: Theme,
    pub language: Language,
    /// ANSI palette preset for the interactive terminal (independent of UI chrome theme).
    #[serde(default)]
    pub terminal_theme: TerminalTheme,
    /// Absolute path to HashiCorp Boundary CLI. Empty / unset = PATH lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary_cli_path: Option<String>,
    /// Absolute path to Teleport `tsh`. Empty / unset = PATH lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teleport_cli_path: Option<String>,
    /// Allow installing unsigned local plugin packages (PP4 Developer Mode).
    #[serde(default)]
    pub plugin_developer_mode: bool,
    /// Official Marketplace base URL (website-hosted API). Empty clears to offline cache-only.
    #[serde(
        default = "default_marketplace_url",
        skip_serializing_if = "Option::is_none"
    )]
    pub marketplace_url: Option<String>,
}

fn default_marketplace_url() -> Option<String> {
    Some("https://runory.app".to_string())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// Named xterm ANSI palette. Each preset has dark + light companions in the UI.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalTheme {
    #[default]
    Runory,
    OneDark,
    TokyoNight,
    Catppuccin,
    Solarized,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "en-US")]
    EnUs,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            language: Language::EnUs,
            terminal_theme: TerminalTheme::Runory,
            boundary_cli_path: None,
            teleport_cli_path: None,
            plugin_developer_mode: false,
            marketplace_url: default_marketplace_url(),
        }
    }
}
