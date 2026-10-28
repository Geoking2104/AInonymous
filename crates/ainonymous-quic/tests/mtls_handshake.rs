//! Test d'intégration du handshake QUIC mTLS ed25519.
//!
//! Prouve, en conditions réelles (vrai endpoint quinn + handshake rustls/ring) :
//!   1. handshake OK quand le client épingle la bonne clé du serveur ;
//!   2. handshake REJETÉ quand la clé épinglée ne correspond pas (preuve que
//!      `PeerKeyVerifier` ne se contente pas d'un `assertion()`) ;
//!   3. connection REJECTED when the authenticated control plane does not
//!      provide a server key pin.

use std::time::Duration;

use ainonymous_quic::{
    create_endpoint, NodeIdentity, QuicListener, QuicSession, SessionConfig, SessionOffer,
};

/// Démarre un endpoint serveur mTLS et une tâche d'acceptation qui lit le token
/// d'auth puis garde la connexion ouverte. Retourne (adresse, handle).
async fn spawn_server(
    server_id: NodeIdentity,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
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

    assert!(
        res.is_ok(),
        "le handshake aurait dû réussir: {:?}",
        res.err()
    );
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
async fn connexion_rejetee_sans_epinglage() {
    let server_id = NodeIdentity::generate();
    let client_id = NodeIdentity::generate();

    let (addr, srv) = spawn_server(server_id).await;

    let client_ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &client_id)
        .await
        .unwrap();
    // An Ed25519 certificate proves possession of some key, not the expected
    // peer's identity. Missing control-plane binding must fail closed.
    let res = QuicSession::connect(
        &client_ep,
        offer_for(addr, None),
        SessionConfig::default(),
        &client_id,
    )
    .await;

    assert!(res.is_err(), "connexion sans épinglage aurait dû échouer");
    srv.abort();
}

/// Régression (constat spike-quic S4) : le listener ne consomme le token
/// qu'APRÈS vérification de la clé client mTLS. Un attaquant qui présente un
/// token valide avec sa propre clé doit être refusé **sans brûler le token** ;
/// le client légitime doit ensuite ouvrir la session avec la même offre.
#[tokio::test]
async fn cle_client_erronee_ne_brule_pas_le_token() {
    use std::sync::{Arc, Mutex};

    let server_id = NodeIdentity::generate();
    let legit = NodeIdentity::generate();
    let attacker = NodeIdentity::generate();

    let listener = Arc::new(
        QuicListener::new("127.0.0.1:0".parse().unwrap(), &server_id)
            .await
            .expect("listener"),
    );
    let addr = listener.local_addr().unwrap();

    // Offre enregistrée par le plan de contrôle pour le client légitime ; le
    // même token est remis aux deux clients (l'attaquant l'a « intercepté »).
    let mut offer = SessionOffer::new(addr, Some((0, 1)));
    offer.peer_pubkey = Some(server_id.public_key_bytes());
    offer.client_pubkey = Some(legit.public_key_bytes());
    listener.register_session(offer.clone());

    // Le handler est la seule preuve qu'une session a été admise.
    let authenticated: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let run = {
        let listener = listener.clone();
        let authenticated = authenticated.clone();
        tokio::spawn(async move {
            listener
                .run(move |_conn, offer| {
                    let authenticated = authenticated.clone();
                    async move {
                        authenticated
                            .lock()
                            .unwrap()
                            .push(offer.session_token.clone());
                    }
                })
                .await
        })
    };

    // 1) Token volé + clé de l'attaquant : refus côté serveur. L'issue côté
    //    client est indifférente (handshake refusé ou connexion fermée après
    //    le refus) ; ce qui compte est que le token ne soit pas consommé.
    let attacker_ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &attacker)
        .await
        .unwrap();
    if let Ok(attacker_session) = QuicSession::connect(
        &attacker_ep,
        offer.clone(),
        SessionConfig::default(),
        &attacker,
    )
    .await
    {
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            attacker_session.connection.closed(),
        )
        .await;
    }
    // Laisser le listener instruire le refus avant la tentative légitime
    // (boucle locale ; même idiome que les tests du spike S4).
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 2) Le client légitime réutilise le MÊME token : il doit être admis.
    let legit_ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &legit)
        .await
        .unwrap();
    let legit_session =
        QuicSession::connect(&legit_ep, offer.clone(), SessionConfig::default(), &legit)
            .await
            .expect("le client légitime doit se connecter");
    let token = legit_session.offer.session_token.clone();

    // 3) Le handler reçoit la session : le token n'a pas été brûlé.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if authenticated.lock().unwrap().iter().any(|t| t == &token) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "token brûlé par la tentative non autorisée : session légitime jamais admise"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    run.abort();
}
