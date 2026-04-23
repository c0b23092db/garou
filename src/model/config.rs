use anyhow::{Context, Result};
use crossterm::style::Color;
use dirs::home_dir;
use image::imageops::FilterType;
use serde::{Deserialize, Deserializer};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// アプリケーションの設定を管理する構造体と、その関連関数
#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    #[serde(default)]
    cache: CacheConfig,
    #[serde(default)]
    display: DisplayConfig,
    #[serde(default)]
    image: ImageConfig,
}

/// 画像の差分表示モードを表す列挙型、アプリケーションの設定を管理する構造体
#[derive(Debug, Clone, Deserialize)]
pub struct ImageConfig {
    #[serde(default)]
    pub diff_mode: ImageDiffMode,
    #[serde(default)]
    pub transport_mode: TransportMode,
    #[serde(default = "default_dirty_ratio")]
    pub dirty_ratio: f32,
    #[serde(default = "default_tile_grid")]
    pub tile_grid: u32,
    #[serde(default = "default_skip_step")]
    pub skip_step: u32,
    #[serde(default = "default_image_extensions")]
    pub extensions: Vec<String>,
    #[serde(default = "default_image_width")]
    pub image_width: u32,
    #[serde(default = "default_image_height")]
    pub image_height: u32,
    #[serde(default, alias = "image_filtertype")]
    pub filter_type: ImageFilterType,
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            diff_mode: ImageDiffMode::All,
            transport_mode: TransportMode::Auto,
            dirty_ratio: default_dirty_ratio(),
            tile_grid: default_tile_grid(),
            skip_step: default_skip_step(),
            extensions: default_image_extensions(),
            image_width: default_image_width(),
            image_height: default_image_height(),
            filter_type: ImageFilterType::Nearest,
        }
    }
}

/// アプリケーションの設定を管理する構造体
#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_lru_size")]
    pub lru_size: usize,
    #[serde(default = "default_cache_max_bytes")]
    pub max_bytes: usize,
    #[serde(default = "default_prefetch_size")]
    pub prefetch_size: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            lru_size: default_cache_lru_size(),
            max_bytes: default_cache_max_bytes(),
            prefetch_size: default_prefetch_size(),
        }
    }
}

/// アプリケーションの設定を管理する構造体
#[derive(Debug, Clone, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_true")]
    pub sidebar: bool,
    #[serde(default = "default_true")]
    pub header: bool,
    #[serde(default = "default_true")]
    pub statusbar: bool,
    #[serde(default = "default_sidebar_size")]
    pub sidebar_size: u16,
    /// プレビュー表示のデバウンス時間 (ms)
    #[serde(default = "default_preview_debounce")]
    pub preview_debounce: u64,
    /// アイドル状態のポーリング間隔 (ms)
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u64,
    /// アイドル状態の先読み間隔 (ms)
    #[serde(default = "default_prefetch_interval")]
    pub prefetch_interval: u64,
    #[serde(
        default = "default_header_bg_color",
        deserialize_with = "deserialize_color"
    )]
    pub header_bg_color: Color,
    #[serde(
        default = "default_header_fg_color",
        deserialize_with = "deserialize_color"
    )]
    pub header_fg_color: Color,
    #[serde(
        default = "default_statusbar_bg_color",
        deserialize_with = "deserialize_color"
    )]
    pub statusbar_bg_color: Color,
    #[serde(
        default = "default_statusbar_fg_color",
        deserialize_with = "deserialize_color"
    )]
    pub statusbar_fg_color: Color,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            sidebar: default_true(),
            header: default_true(),
            statusbar: default_true(),
            sidebar_size: default_sidebar_size(),
            preview_debounce: default_preview_debounce(),
            poll_interval: default_poll_interval(),
            prefetch_interval: default_prefetch_interval(),
            header_bg_color: default_header_bg_color(),
            header_fg_color: default_header_fg_color(),
            statusbar_bg_color: default_statusbar_bg_color(),
            statusbar_fg_color: default_statusbar_fg_color(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    pub cache: CacheConfig,
    pub display: DisplayConfig,
    pub image: ImageConfig,
}

/// 画像の差分表示モードを表す列挙型
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
pub enum ImageDiffMode {
    All,
    #[default]
    Full,
    Half,
}

/// Kitty Graphics Protocol の転送モードを表す列挙型
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportMode {
    /// 実行環境に応じて転送モードを選択する
    #[default]
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
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
pub enum ImageFilterType {
    #[default]
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
    /// 設定ファイルからアプリケーションの設定を読み込む関数
    pub fn load() -> Result<Self> {
        let config_path = home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".config")
            .join("garou")
            .join("config.toml");
        Self::load_from_path(&config_path)
    }

    /// 指定されたパスからアプリケーションの設定を読み込む関数
    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let raw: RawConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config file: {}", path.display()))?;

        Ok(Self {
            cache: raw.cache,
            display: raw.display,
            image: raw.image,
        })
    }
}

fn default_true() -> bool {
    true
}

fn default_preview_debounce() -> u64 {
    100
}

fn default_poll_interval() -> u64 {
    10
}

fn default_prefetch_interval() -> u64 {
    100
}

fn default_dirty_ratio() -> f32 {
    0.10
}

fn default_tile_grid() -> u32 {
    32
}

fn default_skip_step() -> u32 {
    1
}

fn default_sidebar_size() -> u16 {
    20
}

fn default_header_bg_color() -> Color {
    Color::DarkBlue
}
fn default_header_fg_color() -> Color {
    Color::White
}
fn default_statusbar_bg_color() -> Color {
    Color::DarkGrey
}
fn default_statusbar_fg_color() -> Color {
    Color::White
}

fn default_image_width() -> u32 {
    5120
}
fn default_image_height() -> u32 {
    2880
}

fn default_cache_lru_size() -> usize {
    10
}
fn default_prefetch_size() -> usize {
    1
}
fn default_cache_max_bytes() -> usize {
    256 * 1024 * 1024
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

/// デフォルトでサポートする画像拡張子のリストを返す関数
fn default_image_extensions() -> Vec<String> {
    ["png", "jpg", "jpeg", "gif", "webp", "bmp"]
        .iter()
        .map(|ext| ext.to_string())
        .collect()
}
