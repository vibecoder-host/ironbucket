use tower_http::compression::CompressionLayer as TowerCompressionLayer;

pub struct CompressionLayer;

impl CompressionLayer {
    pub fn new() -> TowerCompressionLayer {
        TowerCompressionLayer::new()
    }
}