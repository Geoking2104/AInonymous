/// Un étage du pipeline distribué
#[derive(Debug, Clone)]
pub struct PipelineStage {
    pub node: String,
    pub quic_endpoint: SocketAddr,
    pub layer_start: u32,
    pub layer_end: u32,
    pub is_last: bool,
    /// Plage de couches assignée à ce nœud (pour llama.cpp / pipeline réel)
    pub layer_range: Option<(u32, u32)>,
}

impl PipelineStage {
    pub fn layer_range(&self) -> (u32, u32) {
        self.layer_range.unwrap_or((self.layer_start, self.layer_end))
    }
}
