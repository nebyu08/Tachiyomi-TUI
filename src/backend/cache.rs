use image::DynamicImage;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const MAX_MEMORY_PAGES: usize = 50;
const MAX_DISK_CACHE_MB: u64 = 500;

#[derive(Clone)]
pub struct PageCache {
    inner: Arc<RwLock<PageCacheInner>>,
}

struct PageCacheInner {
    pages: HashMap<String, DynamicImage>,
    access_order: Vec<String>,
    chapter_urls: HashMap<String, Vec<String>>,
    cache_dir: PathBuf,
}

impl PageCache {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tachiyomi-tui")
            .join("pages");

        if let Err(e) = fs::create_dir_all(&cache_dir) {
            eprintln!("Failed to create cache directory: {}", e);
        }

        Self {
            inner: Arc::new(RwLock::new(PageCacheInner {
                pages: HashMap::new(),
                access_order: Vec::new(),
                chapter_urls: HashMap::new(),
                cache_dir,
            })),
        }
    }

    pub async fn get_page(&self, url: &str) -> Option<DynamicImage> {
        let mut inner = self.inner.write().await;

        if inner.pages.contains_key(url) {
            let image = inner.pages.get(url).cloned();
            inner.access_order.retain(|k| k != url);
            inner.access_order.push(url.to_string());
            return image;
        }

        if let Some(image) = inner.load_from_disk(url) {
            inner.insert_memory(url.to_string(), image.clone());
            return Some(image);
        }

        None
    }

    pub async fn insert_page(&self, url: String, image: DynamicImage) {
        let mut inner = self.inner.write().await;
        inner.save_to_disk(&url, &image);
        inner.insert_memory(url, image);
    }

    pub async fn get_chapter_urls(&self, chapter_id: &str) -> Option<Vec<String>> {
        let inner = self.inner.read().await;
        inner.chapter_urls.get(chapter_id).cloned()
    }

    pub async fn insert_chapter_urls(&self, chapter_id: String, urls: Vec<String>) {
        let mut inner = self.inner.write().await;
        inner.chapter_urls.insert(chapter_id, urls);
    }

    pub async fn has_page(&self, url: &str) -> bool {
        let inner = self.inner.read().await;
        if inner.pages.contains_key(url) {
            return true;
        }
        inner.disk_cache_exists(url)
    }
}

impl PageCacheInner {
    fn insert_memory(&mut self, url: String, image: DynamicImage) {
        if self.pages.len() >= MAX_MEMORY_PAGES {
            if let Some(oldest) = self.access_order.first().cloned() {
                self.pages.remove(&oldest);
                self.access_order.remove(0);
            }
        }

        self.access_order.retain(|k| k != &url);
        self.access_order.push(url.clone());
        self.pages.insert(url, image);
    }

    fn url_to_filename(&self, url: &str) -> PathBuf {
        let hash = format!("{:x}", md5_hash(url));
        self.cache_dir.join(hash)
    }

    fn disk_cache_exists(&self, url: &str) -> bool {
        self.url_to_filename(url).exists()
    }

    fn load_from_disk(&self, url: &str) -> Option<DynamicImage> {
        let path = self.url_to_filename(url);
        let bytes = fs::read(&path).ok()?;
        image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()
    }

    fn save_to_disk(&self, url: &str, image: &DynamicImage) {
        self.cleanup_old_cache();

        let path = self.url_to_filename(url);
        if let Ok(mut file) = fs::File::create(&path) {
            let _ = image.write_to(&mut file, image::ImageFormat::Jpeg);
        }
    }

    fn cleanup_old_cache(&self) {
        let max_bytes = MAX_DISK_CACHE_MB * 1024 * 1024;

        let entries: Vec<_> = fs::read_dir(&self.cache_dir)
            .ok()
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let meta = e.metadata().ok()?;
                        let modified = meta.modified().ok()?;
                        Some((e.path(), meta.len(), modified))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let total_size: u64 = entries.iter().map(|(_, size, _)| size).sum();

        if total_size > max_bytes {
            let mut entries = entries;
            entries.sort_by_key(|(_, _, modified)| *modified);

            let mut current_size = total_size;
            for (path, size, _) in entries {
                if current_size <= max_bytes * 80 / 100 {
                    break;
                }
                if fs::remove_file(&path).is_ok() {
                    current_size -= size;
                }
            }
        }
    }
}

fn md5_hash(s: &str) -> u128 {
    let mut hash: u128 = 0;
    for (i, byte) in s.bytes().enumerate() {
        hash = hash.wrapping_add((byte as u128).wrapping_mul(31u128.wrapping_pow(i as u32)));
    }
    hash
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new()
    }
}
