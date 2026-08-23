use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, info};

use ainonymous_quic::{NodeIdentity, SessionOffer, SessionRegistry};
use holochain_client::{
    AdminWebsocket, AppWebsocket, AuthorizeSigningCredentialsPayload, CellInfo, ClientAgentSigner,
    ExternIO, ZomeCallTarget,
};
use holochain_types::prelude::{
    AppBundleSource, InstallAppPayload, RoleSettings, Signal, UnsafeBytes,
};
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
        info!(
            "[hc-connect] 1/5 AdminWebsocket::connect(port={})",
            admin_port
        );
        let admin = AdminWebsocket::connect(("127.0.0.1", admin_port), None).await?;

        info!("[hc-connect] 2/5 issue_app_auth_token(app={})", app_id);
        let issued = admin
            .issue_app_auth_token(app_id.to_string().into())
            .await?;

        info!("[hc-connect] 3/5 AppWebsocket::connect(port={})", app_port);
        let signer = ClientAgentSigner::default();
        let app = AppWebsocket::connect(
            ("127.0.0.1", app_port),
            issued.token,
            signer.clone().into(),
            None,
        )
        .await?;

        info!("[hc-connect] 4/5 authorize_signing_credentials pour chaque cell");
        let mut authorized = 0usize;
        for (role, cells) in app.cached_app_info().cell_info.iter() {
            for cell in cells {
                if let CellInfo::Provisioned(pc) = cell {
                    debug!("[hc-connect]   -> signing creds role={}", role);
                    let creds = admin
                        .authorize_signing_credentials(AuthorizeSigningCredentialsPayload {
                            cell_id: pc.cell_id.clone(),
                            functions: None,
                        })
                        .await?;
                    signer.add_credentials(pc.cell_id.clone(), creds);
                    authorized += 1;
                }
            }
        }
        info!(
            "[hc-connect] 5/5 Connecté ({} cell(s) signable(s))",
            authorized
        );

        let proof_bytes = match membrane_proof {
            Some(cfg) => Some(cfg.to_bytes()?),
            None => None,
        };

        info!(
            "Conducteur Holochain connecté (app='{}', membrane_proof: {})",
            app_id,
            if proof_bytes.is_some() {
                "présent"
            } else {
                "absent"
            }
        );

        Ok(Self {
            app,
            membrane_proof: proof_bytes,
        })
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

        let io = ExternIO::encode(payload)?;
        let out = self
            .app
            .call_zome(
                ZomeCallTarget::RoleName(RoleName::from(role.to_string())),
                ZomeName::from(zome_name.clone()),
                FunctionName::from(func.to_string()),
                io,
            )
            .await?;

        out.decode::<Value>().map_err(|e| {
            anyhow::anyhow!(
                "échec du décodage ExternIO (zome call {}::{}::{}): {}",
                role,
                zome,
                func,
                e
            )
        })
    }

    /// Install and enable a Holochain 0.7 hApp, attaching a membrane proof to
    /// the provisioned role that validates private-network admission.
    pub async fn install_app_with_membrane_proof(
        &self,
        admin: &AdminWebsocket,
        app_id: &str,
        bundle_path: &Path,
        proof_role: &str,
        membrane_proof: Option<Vec<u8>>,
    ) -> Result<()> {
        let roles_settings = membrane_proof.map(|proof| {
            let mut settings = HashMap::new();
            settings.insert(
                RoleName::from(proof_role.to_string()),
                RoleSettings::Provisioned {
                    membrane_proof: Some(Arc::new(UnsafeBytes::from(proof).into())),
                    modifiers: None,
                    init_properties: None,
                },
            );
            settings
        });

        admin
            .install_app(InstallAppPayload {
                source: AppBundleSource::Path(bundle_path.to_path_buf()),
                agent_key: None,
                installed_app_id: Some(app_id.to_string()),
                network_seed: None,
                roles_settings,
                ignore_genesis_failure: false,
                restore_from_dht: false,
            })
            .await?;
        admin.enable_app(app_id.to_string()).await?;
        Ok(())
    }

    pub async fn listen_quic_signals(
        &self,
        registry: SessionRegistry,
        advertise: SocketAddr,
        identity: NodeIdentity,
    ) {
        self.app
            .on_signal(move |sig| {
                let Signal::App {
                    zome_name, signal, ..
                } = sig
                else {
                    return;
                };
                if zome_name.to_string() != INFERENCE_COORDINATOR_ZOME {
                    return;
                }

                if let Ok(qls) = signal.into_inner().decode::<QuicListenerSignal>() {
                    let mut offer = SessionOffer::new(advertise, qls.layer_range);
                    offer.session_token = qls.session_token;
                    offer.next_agent_id = qls.next_agent_id;
                    offer.next_layer_range = qls.next_layer_range;
                    offer.peer_pubkey = Some(identity.public_key_bytes());
                    offer.client_pubkey = qls
                        .requester_pubkey
                        .and_then(|v| <[u8; 32]>::try_from(v).ok());
                    if offer.client_pubkey.is_some() {
                        registry.register(offer);
                    } else {
                        tracing::warn!(
                        "Ignoring unauthenticated QUIC offer: requester_pubkey missing or invalid"
                    );
                    }
                }
            })
            .await;
    }
}
