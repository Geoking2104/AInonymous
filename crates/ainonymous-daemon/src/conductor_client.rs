use std::net::SocketAddr;
use std::path::Path;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, info};

use ainonymous_quic::{NodeIdentity, SessionOffer, SessionRegistry};
use holochain_client::{
    AdminWebsocket, AppWebsocket, AuthorizeSigningCredentialsPayload, CellInfo, ClientAgentSigner,
    ExternIO, ZomeCallTarget,
};
use holochain_types::prelude::Signal;
use holochain_zome_types::prelude::{FunctionName, RoleName, ZomeName};

use crate::config::MembraneProofConfig;

const INFERENCE_COORDINATOR_ZOME: &str = "inference-mesh-coordinator";

#[derive(Debug, Deserialize)]
struct QuicListenerSignal {
    session_token: Vec<u8>,
    #[serde(default)]
    layer_range: Option<(u32, u32)>,
    #[serde(default)]
    next_agent_id: Option<String>,
    #[serde(default)]
    next_layer_range: Option<(u32, u32)>,
    #[serde(default)]
    requester_pubkey: Option<Vec<u8>>,
}

pub struct ConductorClient {
    app: AppWebsocket,
    membrane_proof: Option<Vec<u8>>,
}

impl ConductorClient {
    pub async fn connect(
        admin_port: u16,
        app_port: u16,
        app_id: &str,
        membrane_proof: Option<MembraneProofConfig>,
    ) -> Result<Self> {
        let admin = AdminWebsocket::connect(("127.0.0.1", admin_port), None).await?;
        let issued = admin.issue_app_auth_token(app_id.to_string().into()).await?;

        let signer = ClientAgentSigner::default();
        let app = AppWebsocket::connect(
            ("127.0.0.1", app_port),
            issued.token,
            signer.clone().into(),
            None,
        ).await?;

        for (role, cells) in app.cached_app_info().cell_info.iter() {
            for cell in cells {
                if let CellInfo::Provisioned(pc) = cell {
                    let creds = admin
                        .authorize_signing_credentials(AuthorizeSigningCredentialsPayload {
                            cell_id: pc.cell_id.clone(),
                            functions: None,
                        })
                        .await?;
                    signer.add_credentials(pc.cell_id.clone(), creds);
                }
            }
        }

        let proof_bytes = membrane_proof.and_then(|cfg| cfg.to_bytes().ok());

        info!("Conducteur Holochain connecté (app='{}', membrane_proof: {})",
              app_id, if proof_bytes.is_some() { "présent" } else { "absent" });

        Ok(Self { app, membrane_proof: proof_bytes })
    }

    pub fn membrane_proof(&self) -> Option<&[u8]> {
        self.membrane_proof.as_deref()
    }

    /// Appelle un zome en injectant automatiquement la membrane_proof si elle existe
    /// et que le payload ne la contient pas déjà.
    pub async fn call_zome_with_proof(
        &self,
        role: &str,
        zome: &str,
        func: &str,
        mut payload: Value,
    ) -> Result<Value> {
        if self.membrane_proof.is_some() && payload.get("membrane_proof").is_none() {
            if let Some(proof) = &self.membrane_proof {
                payload["membrane_proof"] = serde_json::to_value(proof)?;
            }
        }

        self.call_zome_json(role, zome, func, payload).await
    }

    pub async fn call_zome_json(
        &self,
        role: &str,
        zome: &str,
        func: &str,
        payload: Value,
    ) -> Result<Value> {
        let zome_name = if zome == "coordinator" || zome == "integrity" {
            format!("{role}-{zome}")
        } else {
            zome.to_string()
        };

        let io = ExternIO::encode(payload)?;
        let out = self.app
            .call_zome(
                ZomeCallTarget::RoleName(RoleName::from(role.to_string())),
                ZomeName::from(zome_name.clone()),
                FunctionName::from(func.to_string()),
                io,
            )
            .await?;

        out.decode::<Value>()
    }

    /// Installe une happ avec un Membrane Proof (pour consortiums privés)
    pub async fn install_app_with_membrane_proof(
        &self,
        admin: &mut AdminWebsocket,
        app_id: &str,
        bundle_path: &Path,
        membrane_proof: Option<Vec<u8>>,
    ) -> Result<()> {
        use holochain_types::prelude::{AppBundle, InstallAppPayload, MembraneProof};

        let bundle = AppBundle::decode(std::fs::read(bundle_path)?)?;
        let proof = membrane_proof.map(|bytes| MembraneProof::from(bytes));

        let payload = InstallAppPayload {
            installed_app_id: Some(app_id.to_string()),
            agent_key: None,
            membrane_proofs: proof.map(|p| vec![("default".into(), p)]).unwrap_or_default(),
            bundle,
            network_seed: None,
        };

        admin.install_app(payload).await?;
        info!("App '{}' installée avec Membrane Proof", app_id);
        Ok(())
    }

    pub async fn listen_quic_signals(
        &self,
        registry: SessionRegistry,
        advertise: SocketAddr,
        identity: NodeIdentity,
    ) {
        self.app.on_signal(move |sig| {
            let Signal::App { zome_name, signal, .. } = sig else { return; };
            if zome_name.to_string() != INFERENCE_COORDINATOR_ZOME { return; }

            if let Ok(qls) = signal.into_inner().decode::<QuicListenerSignal>() {
                let mut offer = SessionOffer::new(advertise, qls.layer_range);
                offer.session_token = qls.session_token;
                offer.next_agent_id = qls.next_agent_id;
                offer.next_layer_range = qls.next_layer_range;
                offer.peer_pubkey = Some(identity.public_key_bytes());
                offer.client_pubkey = qls.requester_pubkey
                    .and_then(|v| <[u8; 32]>::try_from(v).ok());
                registry.register(offer);
            }
        }).await;
    }
}