/// Retourne la taille réelle d'un fichier GGUF en Go.
/// Retourne une valeur par défaut (13.0) si le fichier n'existe pas ou est illisible.
pub fn get_model_size_gb(model_path: &std::path::Path) -> f32 {
    match std::fs::metadata(model_path) {
        Ok(metadata) => {
            let size_bytes = metadata.len() as f32;
            let size_gb = size_bytes / (1024.0 * 1024.0 * 1024.0);
            debug!("Taille du modèle {:?} : {:.2} Go", model_path, size_gb);
            size_gb
        }
        Err(e) => {
            warn!("Impossible de lire la taille du modèle {:?}: {}. Utilisation de la valeur par défaut (13 Go).", model_path, e);
            13.0
        }
    }
}
