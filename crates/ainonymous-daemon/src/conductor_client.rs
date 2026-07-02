//! Client conducteur Holochain réel via `holochain_client::AppWebsocket`.
//!
//! Flux de connexion (mode `holochain.backend = "conductor"`):
//! 1. `AdminWebsocket` → émettre un token d'authentification pour l'app.
//! 2. `AppWebsocket::connect` avec ce token + un `ClientAgentSigner` en mémoire.
//! 3. Pour chaque cell provisionnée, `authorize_signing_credentials` (admin)
//!    puis on ajoute les credentials au signer partagé → les appels de zome
//!    sont signés localement (pas besoin de lair côté client).
//!
//! Les appels passent par `call_zome_json` : on encode/décode en MessagePack via
//! `ExternIO`, avec `serde_json::Value` à la frontière (même convention que le
//! reste du daemon).
//!
//! Convention de nommage : les appelants passent `zome = "coordinator"` ou
//! `"integrity"` ; le vrai nom de zome dans les manifests est `"{role}-{zome}"`
//! (ex. `inference-mesh-coordinator`). On applique ce mapping ici.

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

/// Nom du zome coordinateur qui émet `QuicListenerSignal`.
const INFERENCE_COORDINATOR_ZOME: &str = "inference-mesh-coordinator";

/// Copie côté daemon du signal émis par le zome `negotiate_quic_session`.
///
/// Seuls les champs utiles au listener sont désérialisés ; `requestor`
/// (`AgentPubKey`, encodé en bytes) est ignoré (serde ignore les champs inconnus).
#[derive(Debug, Deserialize)]
struct QuicListenerSignal {
    /// Token de session (32 octets aléatoires) — encodé en séquence côté zome.
    session_token: Vec<u8>,
    #[serde(default)]
    layer_range: Option<(u32, u32)>,
    /// Next-hop de la chaîne pipeline (propagé par le coordinateur).
    #[serde(default)]
    next_agent_id: Option<String>,
    #[serde(default)]
    next_layer_range: Option<(u32, u32)>,
}

/// Connexion vivante à un conducteur Holochain pour une app installée.
pub struct ConductorClient {
    app: AppWebsocket,
}

impl ConductorClient {
    /// Se connecte au conducteur (admin + app) et autorise la signature des
    /// appels de zome pour toutes les cells provisionnées de l'app.
    pub async fn connect(admin_port: u16, app_port: u16, app_id: &str) -> Result<Self> {
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

        // Autoriser la signature pour chaque cell provisionnée de l'app.
        let mut authorized = 0usize;
        for (role, cells) in app.cached_app_info().cell_info.iter() {
            for cell in cells {
                if let CellInfo::Provisioned(pc) = cell {
                    let creds = admin
                        .authorize_signing_credentials(AuthorizeSigningCredentialsPayload {
                            cell_id: pc.cell_id.clone(),
                            functions: None, // toutes les fonctions
                        })
                        .await
                        .map_err(|e| anyhow!("autorisation signature (role '{role}'): {e}"))?;
                    signer.add_credentials(pc.cell_id.clone(), creds);
                    authorized += 1;
                }
            }
        }

        info!(
            "Conducteur Holochain connecté (app='{}', agent={:?}, {} cell(s) signables)",
            app_id, app.my_pub_key, authorized
        );

        Ok(Self { app })
    }

    /// Appelle une fonction de zome et convertit le résultat en JSON.
    ///
    /// `role` = nom de rôle de l'app (= nom de DNA : `inference-mesh`, …).
    /// `zome` = `"coordinator"` / `"integrity"` (mappé vers `"{role}-{zome}"`),
    /// ou un nom de zome complet.
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

    /// Enregistre un handler qui, à réception d'un `QuicListenerSignal` émis par
    /// le zome `negotiate_quic_session`, publie l'offre de session correspondante
    /// dans le registre du listener QUIC local (moitié « entrante » de la
    /// négociation, pendant DHT du POST REST `/mesh/session/negotiate`).
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
                match signal.into_inner().decode::<QuicListenerSignal>() {
                    Ok(qls) => {
                        let mut offer = SessionOffer::new(advertise, qls.layer_range);
                        offer.session_token = qls.session_token;
                        offer.next_agent_id = qls.next_agent_id;
                        offer.next_layer_range = qls.next_layer_range;
                        offer.peer_pubkey = Some(identity.public_key_bytes());
                        registry.register(offer);
                        info!(
                            "Session QUIC entrante enregistrée via signal Holochain (couches {:?})",
                            qls.layer_range
                        );
                    }
                    Err(e) => debug!("Signal ignoré (décodage QuicListenerSignal): {e}"),
                }
            })
            .await;
    }
}
