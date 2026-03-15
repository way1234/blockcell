use blockcell_tools::ResponseCacheOps;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Per-session cache for large list/table responses.
///
/// When the LLM returns a long numbered/bulleted list, storing the full text in history
/// causes exponential token growth across turns. This cache stores the content separately
/// and replaces the history entry with a compact stub. The LLM can call `session_recall`
/// to retrieve the full content when the user references a specific item.
#[derive(Clone)]
pub struct ResponseCache {
    inner: Arc<Mutex<ResponseCacheInner>>,
}

struct ResponseCacheInner {
    /// session_key → ref_id → CacheEntry
    data: HashMap<String, HashMap<String, CacheEntry>>,
    /// Maximum cached entries per session (evicts oldest on overflow).
    max_per_session: usize,
}

struct CacheEntry {
    content: String,
    #[allow(dead_code)]
    item_count: usize,
    created_at: i64,
}

impl ResponseCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResponseCacheInner {
                data: HashMap::new(),
                max_per_session: 10,
            })),
        }
    }

    /// If `content` qualifies as a cacheable list/table, stores it and returns a compact stub.
    /// Returns `None` if the content does not meet the caching threshold.
    pub fn maybe_cache_and_stub(&self, session_key: &str, content: &str) -> Option<String> {
        if !Self::is_cacheable(content) {
            return None;
        }
        let items = Self::extract_list_items(content);
        if items.len() < 5 {
            return None;
        }

        let ref_id = Self::generate_ref_id(session_key);
        let preview = items
            .iter()
            .take(3)
            .enumerate()
            .map(|(i, item)| {
                let trimmed: String = item.chars().take(100).collect();
                format!("{}. {}", i + 1, trimmed)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let stub = format!(
            "[已缓存{}条结果，ID: ref:{}]\n{}\n...（共{}条，使用 session_recall 工具获取完整内容）",
            items.len(),
            ref_id,
            preview,
            items.len()
        );

        let entry = CacheEntry {
            content: content.to_string(),
            item_count: items.len(),
            created_at: chrono::Utc::now().timestamp(),
        };

        let mut inner = self.inner.lock().unwrap();
        let max_per_session = inner.max_per_session;
        let session_cache = inner
            .data
            .entry(session_key.to_string())
            .or_default();

        // Evict oldest entry if at capacity
        if session_cache.len() >= max_per_session {
            if let Some(oldest_key) = session_cache
                .iter()
                .min_by_key(|(_, e)| e.created_at)
                .map(|(k, _)| k.clone())
            {
                session_cache.remove(&oldest_key);
            }
        }

        session_cache.insert(ref_id.clone(), entry);
        debug!(
            session_key,
            ref_id = %ref_id,
            item_count = items.len(),
            "Cached large list response"
        );

        Some(stub)
    }

    /// Retrieve cached content by ref_id (with or without "ref:" prefix).
    pub fn recall(&self, session_key: &str, ref_id: &str) -> Option<String> {
        let bare_id = ref_id.strip_prefix("ref:").unwrap_or(ref_id);
        let inner = self.inner.lock().unwrap();
        inner
            .data
            .get(session_key)
            .and_then(|m| m.get(bare_id))
            .map(|e| e.content.clone())
    }

    /// Remove all cache entries for a session (e.g. on session reset).
    pub fn clear_session(&self, session_key: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.data.remove(session_key);
    }

    // ──────────────────────────────────────────────
    // Internal helpers
    // ──────────────────────────────────────────────

    /// Content is cacheable when it is long enough and contains a list.
    fn is_cacheable(content: &str) -> bool {
        content.chars().count() > 800
    }

    /// Extract list items from a numbered or bulleted list.
    /// Handles: `1. item`, `- item`, `* item`, `• item`
    fn extract_list_items(content: &str) -> Vec<String> {
        let mut items = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Numbered: "1. " / "1) "
            if let Some(rest) = Self::strip_numbered_prefix(trimmed) {
                if !rest.is_empty() {
                    items.push(rest.to_string());
                    continue;
                }
            }
            // Bulleted: "- " / "* " / "• "
            if let Some(rest) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("• "))
            {
                if !rest.is_empty() {
                    items.push(rest.to_string());
                }
            }
        }
        items
    }

    fn strip_numbered_prefix(s: &str) -> Option<&str> {
        let mut idx = 0;
        for c in s.chars() {
            if c.is_ascii_digit() {
                idx += c.len_utf8();
            } else {
                break;
            }
        }
        if idx == 0 {
            return None;
        }
        let rest = &s[idx..];
        // Accept ". " or ") "
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            Some(r)
        } else {
            None
        }
    }

    /// Generate a short deterministic+random ref_id from session_key + timestamp.
    fn generate_ref_id(session_key: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let mut hasher = DefaultHasher::new();
        session_key.hash(&mut hasher);
        ts.hash(&mut hasher);
        let h = hasher.finish();
        // 8 lowercase hex chars
        format!("{:08x}", h & 0xFFFF_FFFF)
    }
}

impl Default for ResponseCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseCacheOps for ResponseCache {
    fn recall_json(&self, session_key: &str, ref_id: &str) -> String {
        match self.recall(session_key, ref_id) {
            Some(content) => serde_json::json!({
                "ref_id": ref_id,
                "content": content,
                "status": "found"
            })
            .to_string(),
            None => serde_json::json!({
                "ref_id": ref_id,
                "error": "未找到对应的缓存内容，可能已过期或 ID 不正确",
                "status": "not_found"
            })
            .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_cacheable_short() {
        let cache = ResponseCache::new();
        let result = cache.maybe_cache_and_stub("s1", "短内容");
        assert!(result.is_none());
    }

    #[test]
    fn test_not_cacheable_no_list() {
        let cache = ResponseCache::new();
        // Long but no list items
        let content = "这是一段很长的文本。".repeat(100);
        let result = cache.maybe_cache_and_stub("s1", &content);
        assert!(result.is_none());
    }

    #[test]
    fn test_numbered_list_cached() {
        let cache = ResponseCache::new();
        let content = (1..=20)
            .map(|i| format!("{}. 这是第{}条内容，包含足够多的文字描述以确保达到缓存阈值。本条目记录了系统中的一个重要数据项目。", i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let result = cache.maybe_cache_and_stub("sess_abc", &content);
        assert!(result.is_some(), "content len={}", content.chars().count());
        let stub = result.unwrap();
        assert!(stub.contains("ref:"));
        assert!(stub.contains("20条"));
    }

    #[test]
    fn test_recall_by_id() {
        let cache = ResponseCache::new();
        let content = (1..=20)
            .map(|i| format!("{}. 条目 {} 的详细内容说明，包含足够多的文字描述以确保达到800字符缓存阈值，用于验证recall功能正确性。", i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let stub = cache.maybe_cache_and_stub("sess_xyz", &content).unwrap();
        // Extract ref_id from stub
        let ref_id = stub
            .split("ref:")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        let recalled = cache.recall("sess_xyz", ref_id).unwrap();
        assert_eq!(recalled, content);
    }

    #[test]
    fn test_recall_with_prefix() {
        let cache = ResponseCache::new();
        let content = (1..=20)
            .map(|i| format!("{}. 项目 {} 说明文字，包含足够多的文字描述以确保达到800字符缓存阈值，用于验证带ref前缀的recall功能。", i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let stub = cache.maybe_cache_and_stub("s2", &content).unwrap();
        let bare_id = stub
            .split("ref:")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        // recall_json with "ref:" prefix
        let json_str = cache.recall_json("s2", &format!("ref:{}", bare_id));
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["status"], "found");
    }

    #[test]
    fn test_extract_list_items() {
        let content =
            "1. Apple\n2. Banana\n3. Cherry\n- Dog\n* Elephant\n• Frog\nNot a list item";
        let items = ResponseCache::extract_list_items(content);
        assert_eq!(items.len(), 6);
        assert_eq!(items[0], "Apple");
        assert_eq!(items[3], "Dog");
    }

    #[test]
    fn test_session_isolation() {
        let cache = ResponseCache::new();
        let content = (1..=20)
            .map(|i| format!("{}. 内容 {}，包含足够多的文字描述以确保达到800字符缓存阈值，用于验证会话隔离功能，不同会话的数据不应互相访问。", i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let stub = cache.maybe_cache_and_stub("session_A", &content).unwrap();
        let ref_id = stub
            .split("ref:")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        // Different session should not find it
        assert!(cache.recall("session_B", ref_id).is_none());
        // Same session finds it
        assert!(cache.recall("session_A", ref_id).is_some());
    }
}
