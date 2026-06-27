//! Test d'intégration du handshake QUIC mTLS ed25519.
//!
//! Prouve, en conditions réelles (vrai endpoint quinn + handshake rustls/ring) :
//!   1. handshake OK quand le client épingle la bonne clé du serveur ;
//!   2. handshake REJETÉ quand la clé épinglée ne correspond pas (preuve que
//!      `PeerKeyVerifier` ne se contente pas d'un `assertion()`) ;
//!   3. handshake OK sans épinglage (peer_pubkey = None) mais cert ed25519 valide.

use std::time::Duration;

use ainonymous_quic::{
    create_endpoint, NodeIdentity, QuicSession, SessionConfig, SessionOffer,
};

/// Démarre un endpoint serveur mTLS et une tâche d'acceptation qui lit le token
/// d'auth puis garde la connexion ouverte. Retourne (adresse, handle).
async fn spawn_server(server_id: NodeIdentity) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &server_id)
        .await
        .expect("endpoint serveur");
    let addr = ep.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        if let Some(incoming) = ep.accept().await {
            if let Ok(conn) = incoming.await {
                // Le client ouvre un uni-stream et y écrit le token de session.
                if let Ok(mut s) = conn.accept_uni().await {
                    let _ = s.read_to_end(64).await;
                }
                // Garder la connexion vivante un court instant.
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    });
    (addr, handle)
}

fn offer_for(addr: std::net::SocketAddr, pinned: Option<[u8; 32]>) -> SessionOffer {
    let mut offer = SessionOffer::new(addr, Some((0, 1)));
    offer.peer_pubkey = pinned;
    offer
}

#[tokio::test]
async fn handshake_ok_avec_cle_epinglee_correcte() {
    let server_id = NodeIdentity::generate();
    let client_id = NodeIdentity::generate();
    let server_pk = server_id.public_key_bytes();

    let (addr, srv) = spawn_server(server_id).await;

    let client_ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &client_id)
        .await
        .unwrap();
    let res = QuicSession::connect(
        &client_ep,
        offer_for(addr, Some(server_pk)),
        SessionConfig::default(),
        &client_id,
    )
    .await;

    assert!(res.is_ok(), "le handshake aurait dû réussir: {:?}", res.err());
    srv.abort();
}

#[tokio::test]
async fn handshake_rejete_avec_cle_epinglee_erronee() {
    let server_id = NodeIdentity::generate();
    let client_id = NodeIdentity::generate();

    let (addr, srv) = spawn_server(server_id).await;

    let client_ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &client_id)
        .await
        .unwrap();
    // Clé épinglée volontairement fausse : le cert serveur ne la portera pas.
    let res = QuicSession::connect(
        &client_ep,
        offer_for(addr, Some([0xAB; 32])),
        SessionConfig::default(),
        &client_id,
    )
    .await;

    assert!(
        res.is_err(),
        "le handshake aurait dû échouer (mauvaise clé épinglée)"
    );
    srv.abort();
}

#[tokio::test]
async fn handshake_ok_sans_epinglage() {
    let server_id = NodeIdentity::generate();
    let client_id = NodeIdentity::generate();

    let (addr, srv) = spawn_server(server_id).await;

    let client_ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &client_id)
        .await
        .unwrap();
    // peer_pubkey = None : on accepte tout cert ed25519 valide (possession prouvée).
    let res = QuicSession::connect(
        &client_ep,
        offer_for(addr, None),
        SessionConfig::default(),
        &client_id,
    )
    .await;

    assert!(res.is_ok(), "handshake sans épinglage: {:?}", res.err());
    srv.abort();
}
