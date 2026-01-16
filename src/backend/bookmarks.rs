use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use super::mangadex::Manga;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Bookmarks {
    pub manga_ids: HashSet<String>,
    #[serde(default)]
    pub manga_cache: Vec<BookmarkedManga>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkedManga {
    pub id: String,
    pub title: String,
    pub author: String,
    pub status: String,
    pub description: String,
    pub cover_url: String,
}

impl From<&Manga> for BookmarkedManga {
    fn from(manga: &Manga) -> Self {
        BookmarkedManga {
            id: manga.id.clone(),
            title: manga.title.clone(),
            author: manga.author.clone(),
            status: manga.status.clone(),
            description: manga.description.clone(),
            cover_url: manga.cover_url.clone(),
        }
    }
}

impl From<&BookmarkedManga> for Manga {
    fn from(bm: &BookmarkedManga) -> Self {
        Manga {
            id: bm.id.clone(),
            title: bm.title.clone(),
            author: bm.author.clone(),
            artist: String::new(),
            status: bm.status.clone(),
            description: bm.description.clone(),
            cover_url: bm.cover_url.clone(),
        }
    }
}

fn get_bookmarks_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tachiyomi-tui");
    
    fs::create_dir_all(&config_dir).ok();
    config_dir.join("bookmarks.json")
}

impl Bookmarks {
    pub fn load() -> Self {
        let path = get_bookmarks_path();
        
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(bookmarks) = serde_json::from_str(&content) {
                    return bookmarks;
                }
            }
        }
        
        Bookmarks::default()
    }

    pub fn save(&self) {
        let path = get_bookmarks_path();
        if let Ok(content) = serde_json::to_string_pretty(self) {
            fs::write(path, content).ok();
        }
    }

    pub fn add(&mut self, manga: &Manga) {
        self.manga_ids.insert(manga.id.clone());
        
        // Update cache if not already present
        if !self.manga_cache.iter().any(|m| m.id == manga.id) {
            self.manga_cache.push(BookmarkedManga::from(manga));
        }
        
        self.save();
    }

    pub fn remove(&mut self, manga_id: &str) {
        self.manga_ids.remove(manga_id);
        self.manga_cache.retain(|m| m.id != manga_id);
        self.save();
    }

    pub fn is_bookmarked(&self, manga_id: &str) -> bool {
        self.manga_ids.contains(manga_id)
    }

    pub fn toggle(&mut self, manga: &Manga) -> bool {
        if self.is_bookmarked(&manga.id) {
            self.remove(&manga.id);
            false
        } else {
            self.add(manga);
            true
        }
    }

    pub fn get_bookmarked_manga(&self) -> Vec<Manga> {
        self.manga_cache.iter().map(Manga::from).collect()
    }
}
