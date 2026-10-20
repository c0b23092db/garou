use anyhow::{Context, Result};
use crossterm::style::Color;
use dirs::home_dir;
use image::imageops::FilterType;
use serde::{Deserialize, Deserializer};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::keymap::KeyMap;

/// 既定の設定（config/config.toml を埋め込む）
const DEFAULT_CONFIG: &str = include_str!("../../config/config.toml");

/// アプリケーションの設定を管理する構造体と、その関連関数
#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    cache: CacheConfig,
    display: DisplayConfig,
    image: ImageConfig,
}

/// 画像の差分表示モードを表す列挙型、アプリケーションの設定を管理する構造体
#[derive(Debug, Clone, Deserialize)]
pub struct ImageConfig {
    pub diff_mode: ImageDiffMode,
    pub transport_mode: TransportMode,
    pub dirty_ratio: f32,
    pub tile_grid: u32,
    pub skip_step: u32,
    pub extensions: Vec<String>,
    pub image_width: u32,
    pub image_height: u32,
    #[serde(alias = "image_filtertype")]
    pub filter_type: ImageFilterType,
}

/// アプリケーションの設定を管理する構造体
#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub lru_size: usize,
    pub max_bytes: usize,
    pub prefetch_size: usize,
}

/// アプリケーションの設定を管理する構造体
#[derive(Debug, Clone, Deserialize)]
pub struct DisplayConfig {
    pub sidebar: bool,
    pub header: bool,
    pub statusbar: bool,
    pub sidebar_size: u16,
    /// プレビュー表示のデバウンス時間 (ms)
    pub preview_debounce: u64,
    /// アイドル状態のポーリング間隔 (ms)
    pub poll_interval: u64,
    /// アイドル状態の先読み間隔 (ms)
    pub prefetch_interval: u64,
    #[serde(deserialize_with = "deserialize_color")]
    pub header_bg_color: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub header_fg_color: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub statusbar_bg_color: Color,
    #[serde(deserialize_with = "deserialize_color")]
    pub statusbar_fg_color: Color,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub cache: CacheConfig,
    pub display: DisplayConfig,
    pub image: ImageConfig,
    pub keys: KeyMap,
}

/// 画像の差分表示モードを表す列挙型
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ImageDiffMode {
    All,
    Full,
    Half,
}

/// Kitty Graphics Protocol の転送モードを表す列挙型
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportMode {
    /// 実行環境に応じて転送モードを選択する
    Auto,
    /// t=d: payload を直接送信する
    #[serde(alias = "d")]
    Direct,
    /// t=f: ファイルパス経由
    #[serde(alias = "f")]
    File,
    /// t=t: 一時ファイル経由
    #[serde(alias = "t")]
    TempFile,
    /// t=s: 共有メモリ経由（非対応環境では direct にフォールバック）
    #[serde(alias = "s")]
    SharedMemory,
}

/// 画像リサイズ時の補間フィルタ
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ImageFilterType {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

impl ImageFilterType {
    pub fn as_filter_type(self) -> FilterType {
        match self {
            Self::Nearest => FilterType::Nearest,
            Self::Triangle => FilterType::Triangle,
            Self::CatmullRom => FilterType::CatmullRom,
            Self::Gaussian => FilterType::Gaussian,
            Self::Lanczos3 => FilterType::Lanczos3,
        }
    }
}

/// アプリケーションの設定を管理する構造体と、その関連関数
impl AppConfig {
    /// ~/.config/garou/ の config.toml と keymap.toml を読み込む関数
    pub fn load() -> Result<Self> {
        let dir = home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".config")
            .join("garou");
        let mut config = Self::load_from_path(&dir.join("config.toml"))?;
        config.keys = KeyMap::load_from_path(&dir.join("keymap.toml"))?;
        Ok(config)
    }

    /// 指定されたパスから設定を読み込む関数（キーマップは既定値）。ファイルがなければ既定値
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let content = if path.exists() {
            fs::read_to_string(path)
                .with_context(|| format!("failed to read config file: {}", path.display()))?
        } else {
            String::new()
        };
        let raw = parse_config(&content)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;

        Ok(Self {
            cache: raw.cache,
            display: raw.display,
            image: raw.image,
            keys: KeyMap::default(),
        })
    }
}

/// 既定値に設定を上書きして読み込む。書いた項目だけがセクション内で既定値を置き換える
fn parse_config(content: &str) -> Result<RawConfig> {
    let mut merged: toml::Table = toml::from_str(DEFAULT_CONFIG)?;
    let overrides: toml::Table = toml::from_str(content)?;
    for (key, value) in overrides {
        match (merged.get_mut(&key).and_then(toml::Value::as_table_mut), value) {
            (Some(base), toml::Value::Table(section)) => base.extend(section),
            (_, value) => {
                merged.insert(key, value);
            }
        }
    }
    Ok(merged.try_into()?)
}

/// 色指定の文字列を解析して `Color` 型に変換する関数
fn deserialize_color<'de, D>(deserializer: D) -> Result<Color, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_color(&value).ok_or_else(|| {
        serde::de::Error::custom(format!(
            "invalid color '{}'. Use a named color (black, dark_grey, grey, white, red, dark_red, green, dark_green, yellow, dark_yellow, blue, dark_blue, magenta, dark_magenta, cyan, dark_cyan) or RGB formats (#RRGGBB, rgb(r,g,b), r,g,b)",
            value
        ))
    })
}

/// 色指定の文字列を解析して `Color` 型に変換する関数
fn parse_color(value: &str) -> Option<Color> {
    parse_named_color(value)
        .or_else(|| parse_hex_rgb_color(value))
        .or_else(|| parse_rgb_function_color(value))
        .or_else(|| parse_csv_rgb_color(value))
}

/// 名前付き色の指定を解析する関数
fn parse_named_color(value: &str) -> Option<Color> {
    let key = value.trim().to_ascii_lowercase().replace('-', "_");
    match key.as_str() {
        "black" => Some(Color::Black),
        "dark_grey" | "dark_gray" => Some(Color::DarkGrey),
        "grey" | "gray" => Some(Color::Grey),
        "white" => Some(Color::White),
        "red" => Some(Color::Red),
        "dark_red" => Some(Color::DarkRed),
        "green" => Some(Color::Green),
        "dark_green" => Some(Color::DarkGreen),
        "yellow" => Some(Color::Yellow),
        "dark_yellow" => Some(Color::DarkYellow),
        "blue" => Some(Color::Blue),
        "dark_blue" => Some(Color::DarkBlue),
        "magenta" => Some(Color::Magenta),
        "dark_magenta" => Some(Color::DarkMagenta),
        "cyan" => Some(Color::Cyan),
        "dark_cyan" => Some(Color::DarkCyan),
        _ => None,
    }
}

/// 16進数RGB形式の色指定を解析する関数
fn parse_hex_rgb_color(value: &str) -> Option<Color> {
    let raw = value.trim();
    let hex = raw.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb { r, g, b })
}

/// RGB関数形式の色指定を解析する関数
fn parse_rgb_function_color(value: &str) -> Option<Color> {
    let raw = value.trim();
    let lower = raw.to_ascii_lowercase();
    if !lower.starts_with("rgb(") || !lower.ends_with(')') {
        return None;
    }

    let inner = &raw[4..raw.len().saturating_sub(1)];
    parse_rgb_components(inner)
}

/// RGBコンポーネントをカンマ区切りで解析する関数
fn parse_csv_rgb_color(value: &str) -> Option<Color> {
    let raw = value.trim();
    if !raw.contains(',') {
        return None;
    }
    parse_rgb_components(raw)
}

/// RGBコンポーネントをカンマ区切りで解析する関数
fn parse_rgb_components(text: &str) -> Option<Color> {
    let mut parts = text.split(',').map(|p| p.trim());
    let r = parts.next()?.parse::<u8>().ok()?;
    let g = parts.next()?.parse::<u8>().ok()?;
    let b = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() {
        return None;
    }

    Some(Color::Rgb { r, g, b })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let raw = parse_config("").unwrap();
        assert_eq!(raw.image.diff_mode, ImageDiffMode::All);
        assert_eq!(raw.cache.max_bytes, 256 * 1024 * 1024);
        assert_eq!(raw.display.header_bg_color, Color::DarkBlue);
    }

    #[test]
    fn overrides_keep_other_defaults_in_section() {
        let raw = parse_config("[image]\ntransport_mode = \"f\"\n").unwrap();
        assert_eq!(raw.image.transport_mode, TransportMode::File);
        assert_eq!(raw.image.tile_grid, 32);
        assert_eq!(raw.cache.lru_size, 10);
    }
}
