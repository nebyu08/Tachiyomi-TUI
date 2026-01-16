use image::DynamicImage;
use reqwest::Error;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Cursor;

const BASE_URL: &str = "https://api.mangadex.org";

#[derive(Debug, Clone)]
pub struct Chapter {
    pub id: String,
    pub chapter: String,
    pub title: String,
    pub volume: Option<String>,
    pub pages: usize,
}

#[derive(Debug, Deserialize)]
struct ChapterResponse {
    data: Vec<ChapterData>,
}

#[derive(Debug, Deserialize)]
struct ChapterData {
    id: String,
    attributes: ChapterAttributes,
}

#[derive(Debug, Deserialize)]
struct ChapterAttributes {
    chapter: Option<String>,
    title: Option<String>,
    volume: Option<String>,
    pages: usize,
    #[serde(rename = "translatedLanguage")]
    translated_language: String,
}

#[derive(Debug, Deserialize)]
struct AtHomeResponse {
    #[serde(rename = "baseUrl")]
    base_url: String,
    chapter: AtHomeChapter,
}

#[derive(Debug, Deserialize)]
struct AtHomeChapter {
    hash: String,
    data: Vec<String>,
    #[serde(rename = "dataSaver")]
    data_saver: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Manga {
    pub id: String,
    pub title: String,
    pub author: String,
    pub artist: String,
    pub status: String,
    pub description: String,
    pub cover_url: String,
}

#[derive(Debug, Deserialize)]
struct MangaResponse {
    data: Vec<MangaData>,
}

#[derive(Debug, Deserialize)]
struct MangaData {
    id: String,
    attributes: MangaAttributes,
    relationships: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
struct MangaAttributes {
    title: HashMap<String, String>,
    status: Option<String>,
    description: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct Relationship {
    #[serde(rename = "type")]
    rel_type: String,
    attributes: Option<RelationshipAttributes>,
}

#[derive(Debug, Deserialize)]
struct RelationshipAttributes {
    name: Option<String>,
    #[serde(rename = "fileName")]
    file_name: Option<String>,
}

fn parse_manga_list(response: MangaResponse) -> Vec<Manga> {
    response
        .data
        .into_iter()
        .map(|m| {
            let mut author = String::new();
            let mut artist = String::new();
            let mut cover_filename = String::new();

            for rel in &m.relationships {
                match rel.rel_type.as_str() {
                    "author" => {
                        if let Some(attrs) = &rel.attributes {
                            author = attrs.name.clone().unwrap_or_default();
                        }
                    }
                    "artist" => {
                        if let Some(attrs) = &rel.attributes {
                            artist = attrs.name.clone().unwrap_or_default();
                        }
                    }
                    "cover_art" => {
                        if let Some(attrs) = &rel.attributes {
                            cover_filename = attrs.file_name.clone().unwrap_or_default();
                        }
                    }
                    _ => {}
                }
            }

            let cover_url = if !cover_filename.is_empty() {
                format!(
                    "https://uploads.mangadex.org/covers/{}/{}",
                    m.id, cover_filename
                )
            } else {
                String::new()
            };

            let title = m.attributes.title
                .get("en")
                .or_else(|| m.attributes.title.values().next())
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());

            let description = m.attributes.description
                .as_ref()
                .and_then(|d| d.get("en").or_else(|| d.values().next()))
                .cloned()
                .unwrap_or_default();

            Manga {
                id: m.id,
                title,
                author,
                artist,
                status: m.attributes.status.unwrap_or_else(|| "Unknown".to_string()),
                description,
                cover_url,
            }
        })
        .collect()
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Tachiyomi-TUI/0.1.0")
        .build()
        .expect("Failed to build HTTP client")
}

pub async fn fetch_cover_image(cover_url: &str) -> Option<DynamicImage> {
    if cover_url.is_empty() {
        return None;
    }

    // Use thumbnail size (256px) for faster loading
    let thumb_url = format!("{}.256.jpg", cover_url);
    
    let client = build_client();
    let response = client.get(&thumb_url).send().await.ok()?;
    let bytes = response.bytes().await.ok()?;
    
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

pub async fn get_recently_updated() -> Result<Vec<Manga>, Error> {
    let url = format!(
        "{}/manga?includes[]=author&includes[]=artist&includes[]=cover_art&order[latestUploadedChapter]=desc&limit=20",
        BASE_URL
    );

    let client = build_client();
    let response: MangaResponse = client.get(&url).send().await?.json().await?;

    Ok(parse_manga_list(response))
}

pub async fn get_popular_now() -> Result<Vec<Manga>, Error> {
    let url = format!(
        "{}/manga?includes[]=author&includes[]=artist&includes[]=cover_art&order[followedCount]=desc&limit=20",
        BASE_URL
    );

    let client = build_client();
    let response: MangaResponse = client.get(&url).send().await?.json().await?;

    Ok(parse_manga_list(response))
}

pub async fn get_manga_chapters(manga_id: &str) -> Result<Vec<Chapter>, Error> {
    let url = format!(
        "{}/manga/{}/feed?translatedLanguage[]=en&order[chapter]=desc&limit=100",
        BASE_URL, manga_id
    );

    let client = build_client();
    let response: ChapterResponse = client.get(&url).send().await?.json().await?;

    let chapters = response
        .data
        .into_iter()
        .filter(|c| c.attributes.pages > 0)
        .map(|c| Chapter {
            id: c.id,
            chapter: c.attributes.chapter.unwrap_or_else(|| "0".to_string()),
            title: c.attributes.title.unwrap_or_else(|| "No Title".to_string()),
            volume: c.attributes.volume,
            pages: c.attributes.pages,
        })
        .collect();

    Ok(chapters)
}

pub async fn get_chapter_pages(chapter_id: &str) -> Option<Vec<String>> {
    let url = format!("{}/at-home/server/{}", BASE_URL, chapter_id);

    let client = build_client();
    let response: AtHomeResponse = client.get(&url).send().await.ok()?.json().await.ok()?;

    let pages = response
        .chapter
        .data_saver
        .into_iter()
        .map(|filename| {
            format!(
                "{}/data-saver/{}/{}",
                response.base_url, response.chapter.hash, filename
            )
        })
        .collect();

    Some(pages)
}

pub async fn fetch_page_image(page_url: &str) -> Option<DynamicImage> {
    let client = build_client();
    let response = client.get(page_url).send().await.ok()?;
    let bytes = response.bytes().await.ok()?;

    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_recently_updated() {
        let result = get_recently_updated().await;
        match &result {
            Ok(_) => {}
            Err(e) => println!("Error: {:?}", e),
        }
        assert!(result.is_ok(), "Failed to fetch recently updated manga");

        let mangas = result.unwrap();
        assert!(!mangas.is_empty(), "No manga returned");

        println!("\n=== Recently Updated Manga (Top 10) ===");
        for (i, manga) in mangas.iter().take(10).enumerate() {
            println!(
                "{}. {} | Author: {} | Status: {}",
                i + 1,
                manga.title,
                manga.author,
                manga.status
            );
        }
    }

    #[tokio::test]
    async fn test_get_popular_now() {
        let result = get_popular_now().await;
        match &result {
            Ok(_) => {}
            Err(e) => println!("Error: {:?}", e),
        }
        assert!(result.is_ok(), "Failed to fetch popular manga");

        let mangas = result.unwrap();
        assert!(!mangas.is_empty(), "No manga returned");

        println!("\n=== Popular Now Manga (Top 10) ===");
        for (i, manga) in mangas.iter().take(10).enumerate() {
            println!(
                "{}. {} | Author: {} | Status: {}",
                i + 1,
                manga.title,
                manga.author,
                manga.status
            );
        }
    }
}
