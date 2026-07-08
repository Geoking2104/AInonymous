use std::net::SocketAddr;

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
    /// Membrane proof fournie pour les réseaux privés (Palier F)
    membrane_proof: Option<Vec<u8>>,
}

impl ConductorClient {
    pub async fn connect(
        admin_port: u16,
        app_port: u16,
        app_id: &str,
        membrane_proof: Option<MembraneProofConfig>,
    ) -> Result<Self> {
        let admin = AdminWebsocket::connect(("127.0.0.1", admin_port), None)
            .await
            .map_err(|e| anyhow!("connexion admin ws (port {admin_port}): {e}"))?;

        let issued = admin
            .issue_app_auth_token(app_id.to_string().into())
            .await
            .map_err(|e| anyhow!("émission token app '{app_id}': {e}"))?;

        let signer = ClientAgentSigner::default();
        let app = AppWebsocket::connect(
            ("127.0.0.1", app_port),
            issued.token,
            signer.clone().into(),
            None,
        )
        .await
        .map_err(|e| anyhow!("connexion app ws (port {app_port}): {e}"))?;

        let mut authorized = 0usize;
        for (role, cells) in app.cached_app_info().cell_info.iter() {
            for cell in cells {
                if let CellInfo::Provisioned(pc) = cell {
                    let creds = admin
                        .authorize_signing_credentials(AuthorizeSigningCredentialsPayload {
                            cell_id: pc.cell_id.clone(),
                            functions: None,
                        })
                        .await
                        .map_err(|e| anyhow!("autorisation signature (role '{role}'): {e}"))?;
                    signer.add_credentials(pc.cell_id.clone(), creds);
                    authorized += 1;
                }
            }
        }

        let proof_bytes = match membrane_proof {
            Some(cfg) => Some(cfg.to_bytes()?),
            None => None,
        };

        info!(
            "Conducteur Holochain connecté (app='{}', {} cell(s) signables, membrane_proof: {})",
            app_id, authorized, if proof_bytes.is_some() { "présent" } else { "absent" }
        );

        Ok(Self { app, membrane_proof: proof_bytes })
    }

    pub fn membrane_proof(&self) -> Option<&[u8]> {
        self.membrane_proof.as_deref()
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

        let io = ExternIO::encode(payload).map_err(|e| anyhow!("encode payload: {e}"))?;

        let out = self
            .app
            .call_zome(
                ZomeCallTarget::RoleName(RoleName::from(role.to_string())),
                ZomeName::from(zome_name.clone()),
                FunctionName::from(func.to_string()),
                io,
            )
            .await
            .map_err(|e| anyhow!("call_zome {role}/{zome_name}/{func}: {e}"))?;

        out.decode::<Value>()
            .map_err(|e| anyhow!("décodage réponse {role}/{zome_name}/{func}: {e}"))
    }

    pub async fn listen_quic_signals(
        &self,
        registry: SessionRegistry,
        advertise: SocketAddr,
        identity: NodeIdentity,
    ) {
        self.app
            .on_signal(move |sig| {
                let Signal::App { zome_name, signal, .. } = sig else { return; };
                if zome_name.to_string() != INFERENCE_COORDINATOR_ZOME {
                    return;
                }
                match signal.into_inner().decode::<QuicListenerSignal>() {
                    Ok(qls) => {
                        let mut offer = SessionOffer::new(advertise, qls.layer_range);
                        offer.session_token = qls.session_token;
                        offer.next_agent_id = qls.next_agent_id;
                        offer.next_layer_range = qls.next_layer_range;
                        offer.peer_pubkey = Some(identity.public_key_bytes());
                        offer.client_pubkey = qls.requester_pubkey
                            .and_then(|v| <[u8; 32]>::try_from(v).ok());
                        registry.register(offer);
                        info!(
                            "Session QUIC entrante via signal Holochain (couches {:?})",
                            qls.layer_range
                        );
                    }
                    Err(e) => debug!("Signal ignoré: {e}"),
                }
            })
            .await;
    }
}