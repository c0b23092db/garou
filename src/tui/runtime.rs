use std::collections::hash_map::Entry;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use super::render::image::RgbaFrame;

#[derive(Debug, Default)]
struct CachedImage {
    bytes: Arc<[u8]>,
    rgba: Option<RgbaFrame>,
}

/// 画像キャッシュを管理する構造体と、その関連関数
#[derive(Debug, Default)]
pub(super) struct ImageCache {
    lru_size: usize,
    max_bytes: usize,
    current_bytes: usize,
    map: HashMap<usize, CachedImage>,
    order: VecDeque<usize>,
}

/// 画像キャッシュを管理する構造体と、その関連関数
impl ImageCache {
    pub(super) fn new(lru_size: usize, max_bytes: usize) -> Self {
        Self {
            lru_size,
            max_bytes,
            current_bytes: 0,
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.lru_size > 0 || self.max_bytes > 0
    }

    pub(super) fn contains(&self, key: usize) -> bool {
        self.map.contains_key(&key)
    }

    pub(super) fn get(&mut self, key: usize) -> Option<Arc<[u8]>> {
        let value = self.map.get(&key).map(|entry| entry.bytes.clone());
        if value.is_some() {
            self.touch(key);
        }
        value
    }

    pub(super) fn get_rgba(&mut self, key: usize) -> Option<RgbaFrame> {
        let value = self.map.get(&key).and_then(|entry| entry.rgba.clone());
        if value.is_some() {
            self.touch(key);
        }
        value
    }

    pub(super) fn insert_rgba(&mut self, key: usize, rgba: RgbaFrame) {
        if let Some(entry) = self.map.get_mut(&key) {
            entry.rgba = Some(rgba);
            self.touch(key);
        }
    }

    pub(super) fn insert(&mut self, key: usize, value: Arc<[u8]>) {
        if !self.enabled() {
            return;
        }

        let new_bytes = value.len();
        let cached = CachedImage {
            bytes: value,
            rgba: None,
        };

        match self.map.entry(key) {
            Entry::Occupied(mut entry) => {
                let old_bytes = entry.get().bytes.len();
                entry.insert(cached);
                self.current_bytes = self.current_bytes + new_bytes - old_bytes;
                self.touch(key);
            }
            Entry::Vacant(entry) => {
                entry.insert(cached);
                self.current_bytes += new_bytes;
                self.order.push_back(key);
            }
        }

        self.evict_if_needed();
    }

    pub(super) fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
        self.current_bytes = 0;
    }

    /// キャッシュにアクセスしたキーを最新にする関数
    fn touch(&mut self, key: usize) {
        if let Some(pos) = self.order.iter().position(|&k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_back(key);
    }

    /// キャッシュが制限を超えている場合、最も古いエントリを削除する関数
    fn evict_if_needed(&mut self) {
        while self.exceeds_limits() {
            if let Some(oldest) = self.order.pop_front() {
                if let Some(removed) = self.map.remove(&oldest) {
                    self.current_bytes = self.current_bytes.saturating_sub(removed.bytes.len());
                }
            } else {
                break;
            }
        }
    }

    /// キャッシュが制限を超えているかどうかを判定する関数
    fn exceeds_limits(&self) -> bool {
        (self.lru_size > 0 && self.map.len() > self.lru_size)
            || (self.max_bytes > 0 && self.current_bytes > self.max_bytes)
    }
}
