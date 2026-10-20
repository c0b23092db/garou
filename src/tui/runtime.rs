use std::collections::hash_map::Entry;
use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::Arc,
};

/// キャッシュ容量計算に使うバイト数
pub(super) trait ByteSize {
    fn byte_size(&self) -> usize;
}

impl ByteSize for Arc<[u8]> {
    fn byte_size(&self) -> usize {
        self.len()
    }
}

/// 件数とバイト数で上限を持つ LRU キャッシュ
#[derive(Debug)]
pub(super) struct ImageCache<K, V> {
    lru_size: usize,
    max_bytes: usize,
    current_bytes: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

/// 画像キャッシュを管理する構造体と、その関連関数
impl<K: Eq + Hash + Clone, V: Clone + ByteSize> ImageCache<K, V> {
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

    pub(super) fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    pub(super) fn get(&mut self, key: &K) -> Option<V> {
        let value = self.map.get(key).cloned();
        if value.is_some() {
            self.touch(key);
        }
        value
    }

    pub(super) fn insert(&mut self, key: K, value: V) {
        if !self.enabled() {
            return;
        }

        let new_bytes = value.byte_size();
        match self.map.entry(key.clone()) {
            Entry::Occupied(mut entry) => {
                let old_bytes = entry.get().byte_size();
                entry.insert(value);
                self.current_bytes = self.current_bytes + new_bytes - old_bytes;
                self.touch(&key);
            }
            Entry::Vacant(entry) => {
                entry.insert(value);
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
    fn touch(&mut self, key: &K) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_back(key.clone());
    }

    /// キャッシュが制限を超えている場合、最も古いエントリを削除する関数
    fn evict_if_needed(&mut self) {
        while self.exceeds_limits() {
            if let Some(oldest) = self.order.pop_front() {
                if let Some(removed) = self.map.remove(&oldest) {
                    self.current_bytes = self.current_bytes.saturating_sub(removed.byte_size());
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

impl ByteSize for super::image_pipeline::PreparedImagePayload {
    fn byte_size(&self) -> usize {
        self.image_data.len()
            + self.encoded_payload.len()
            + self.rgba_frame.as_ref().map_or(0, |f| f.pixels.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(n: usize) -> Arc<[u8]> {
        Arc::from(vec![0u8; n])
    }

    #[test]
    fn evicts_least_recently_used_by_count_and_bytes() {
        let mut cache: ImageCache<&str, Arc<[u8]>> = ImageCache::new(2, 100);
        cache.insert("a", bytes(10));
        cache.insert("b", bytes(10));
        cache.get(&"a"); // a を最新にする
        cache.insert("c", bytes(10)); // 件数超過 → 最古の b を削除
        assert!(cache.contains(&"a") && !cache.contains(&"b") && cache.contains(&"c"));

        cache.insert("d", bytes(95)); // バイト数超過 → 収まるまで古い順に削除
        assert!(!cache.contains(&"a") && !cache.contains(&"c") && cache.contains(&"d"));
        assert_eq!(cache.current_bytes, 95);
    }
}
