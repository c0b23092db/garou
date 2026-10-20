//! キー操作と動作の対応表（keymap.toml）

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};

/// キーに割り当て可能な動作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Quit,
    Open,
    ToggleSidebar,
    ToggleStatusbar,
    ToggleHeader,
    ToggleOverlay,
    RefreshImage,
    RefreshScreen,
    Prev,
    Next,
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    ToggleDir,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    SortNatural,
    SortNaturalDesc,
    SortModified,
    SortModifiedDesc,
    SortSize,
    SortSizeDesc,
    Delete,
    Rename,
}

/// 既定のキーマップ（リポジトリ直下の keymap.toml を埋め込む）
const DEFAULT_KEYMAP: &str = include_str!("../../config/keymap.toml");

/// keymap.toml の内容: 動作ごとのキー表記の一覧
pub type KeyBindings = HashMap<Action, Vec<String>>;

/// キー入力から動作を引く対応表
#[derive(Debug, Clone)]
pub struct KeyMap(HashMap<(KeyCode, KeyModifiers), Action>);

impl Default for KeyMap {
    fn default() -> Self {
        Self::new(&HashMap::new()).expect("default keymap.toml is valid")
    }
}

impl KeyMap {
    /// keymap.toml を読み込む。ファイルがなければ既定値
    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read keymap file: {}", path.display()))?;
        let overrides: KeyBindings = toml::from_str(&content)
            .with_context(|| format!("failed to parse keymap file: {}", path.display()))?;
        Self::new(&overrides).with_context(|| format!("invalid keymap file: {}", path.display()))
    }

    /// 既定値に設定を上書きして対応表を作る。設定した動作は既定キーを置き換える。
    pub fn new(overrides: &KeyBindings) -> Result<Self> {
        let defaults: KeyBindings = toml::from_str(DEFAULT_KEYMAP)?;
        // 上書きした動作を後に入れ、既定キーと重なったら上書き側を優先する
        let bindings = defaults
            .iter()
            .filter(|(action, _)| !overrides.contains_key(action))
            .chain(overrides);
        let mut map = HashMap::new();
        for (action, keys) in bindings {
            for key in keys {
                let parsed = parse_key(key).with_context(|| format!("invalid key '{key}'"))?;
                map.insert(parsed, *action);
            }
        }
        Ok(Self(map))
    }

    pub fn action(&self, key: KeyEvent) -> Option<Action> {
        self.0.get(&normalize(key.code, key.modifiers)).copied()
    }
}

/// 文字キーの大文字小文字は文字自体で表すため SHIFT を取り除く
fn normalize(code: KeyCode, mut mods: KeyModifiers) -> (KeyCode, KeyModifiers) {
    if matches!(code, KeyCode::Char(_)) {
        mods.remove(KeyModifiers::SHIFT);
    }
    (code, mods)
}

/// "ctrl+b", "alt+s", "shift+r", "esc", "+" のようなキー表記を解析する
fn parse_key(text: &str) -> Option<(KeyCode, KeyModifiers)> {
    let (mods_text, key) = if text == "+" {
        ("", "+")
    } else if let Some(mods) = text.strip_suffix("++") {
        (mods, "+")
    } else {
        text.rsplit_once('+').unwrap_or(("", text))
    };

    let mut mods = KeyModifiers::NONE;
    for m in mods_text.split('+').filter(|m| !m.is_empty()) {
        mods |= match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => return None,
        };
    }

    let mut code = match key.to_ascii_lowercase().as_str() {
        "esc" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        f if f.len() > 1
            && f.starts_with('f')
            && let Ok(n) = f[1..].parse::<u8>() =>
        {
            KeyCode::F(n)
        }
        _ => {
            let mut chars = key.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(c)
        }
    };
    if let KeyCode::Char(c) = code
        && mods.contains(KeyModifiers::SHIFT)
    {
        code = KeyCode::Char(c.to_ascii_uppercase());
    }
    Some(normalize(code, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn parses_key_notation() {
        assert_eq!(
            parse_key("ctrl+b"),
            Some((KeyCode::Char('b'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            parse_key("shift+r"),
            Some((KeyCode::Char('R'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("+"),
            Some((KeyCode::Char('+'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("ctrl++"),
            Some((KeyCode::Char('+'), KeyModifiers::CONTROL))
        );
        assert_eq!(parse_key("Esc"), Some((KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(parse_key("F2"), Some((KeyCode::F(2), KeyModifiers::NONE)));
        assert_eq!(
            parse_key("f"),
            Some((KeyCode::Char('f'), KeyModifiers::NONE))
        );
        assert_eq!(parse_key("hyper+x"), None);
        assert_eq!(parse_key("xyz"), None);
    }

    #[test]
    fn default_keymap_binds_every_action_once() {
        let defaults: KeyBindings = toml::from_str(DEFAULT_KEYMAP).unwrap();
        assert_eq!(defaults.len(), 32);
        // 既定キー同士が重なっていない
        let keys: usize = defaults.values().map(Vec::len).sum();
        assert_eq!(KeyMap::default().0.len(), keys);
    }

    #[test]
    fn overrides_win_over_conflicting_defaults() {
        // s は既定で sort_size だが、上書きした quit が優先される
        let map = KeyMap::new(&HashMap::from([(Action::Quit, vec!["s".to_string()])])).unwrap();
        let s = key(KeyCode::Char('s'), KeyModifiers::NONE);
        assert_eq!(map.action(s), Some(Action::Quit));
        assert!(toml::from_str::<KeyBindings>("fly = [\"x\"]").is_err());
    }

    #[test]
    fn overrides_replace_defaults() {
        let map = KeyMap::new(&HashMap::from([(Action::Quit, vec!["x".to_string()])])).unwrap();
        let none = KeyModifiers::NONE;
        assert_eq!(
            map.action(key(KeyCode::Char('x'), none)),
            Some(Action::Quit)
        );
        assert_eq!(map.action(key(KeyCode::Char('q'), none)), None);
        // 大文字は SHIFT 付きで届いても一致する
        assert_eq!(
            map.action(key(KeyCode::Char('R'), KeyModifiers::SHIFT)),
            Some(Action::RefreshScreen)
        );
        assert_eq!(
            map.action(key(KeyCode::Char('s'), KeyModifiers::ALT)),
            Some(Action::ToggleSidebar)
        );
        assert_eq!(
            map.action(key(KeyCode::Char('s'), none)),
            Some(Action::SortSize)
        );
    }
}
