impl HolochainClient {
    /// Appelle une fonction de zome avec une meilleure gestion d'erreur
    pub async fn zome_call(
        &self,
        dna: &str,
        zome: &str,
        function: &str,
        payload: Value,
    ) -> Result<Value> {
        debug!("Zome call: {}::{}::{}", dna, zome, function);

        match &self.backend {
            Backend::Conductor(c) => {
                c.call_zome_json(dna, zome, function, payload)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!("Holochain zome call failed [{}::{}::{}]: {}", dna, zome, function, e)
                    })
            }
            Backend::Static => {
                let resp = self
                    .http
                    .post(format!("{}/zome/{}/{}/{}", self.base_url(), dna, zome, function))
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| anyhow::anyhow!("Static zome call HTTP error: {}", e))?;

                if !resp.status().is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    anyhow::bail!("Static zome call failed [{}::{}::{}]: HTTP {} - {}", 
                        dna, zome, function, resp.status(), body);
                }

                resp.json::<Value>()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to parse static zome response: {}", e))
            }
        }
    }

    /// Émet un warrant de façon non-fatale (ne fait pas crash le daemon si le zome n'existe pas encore)
    pub async fn try_emit_warrant(&self, warrant: &Warrant) -> Result<()> {
        match self.emit_warrant_with_cleanup(warrant).await {
            Ok(_) => {
                info!("Warrant émis avec succès: {:?}", warrant.warrant_type);
                Ok(())
            }
            Err(e) => {
                warn!("Impossible d'émettre le warrant ({:?}): {}. Le zome 'warrants' est peut-être pas encore intégré.", 
                      warrant.warrant_type, e);
                // On ne fait pas crash le daemon
                Ok(())
            }
        }
    }
}
