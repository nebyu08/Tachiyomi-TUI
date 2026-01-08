// 1.get recently updated code
// 2.get popular now 
struct Manga {
    id: String,
    title: String,
    author: String,
    artist: String,
    status: String,
    description: String,
    cover_url: String,
}

async fn get_recently_updated() -> Result<Vec<Manga>, Error> {
    let url=
    // Implementation goes here
}

async fn get_popular_now() -> Result<Vec<Manga>, Error> {
    // Implementation goes here
}
