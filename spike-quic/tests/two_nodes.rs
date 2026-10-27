//! S4 / #32 : deux nœuds QUIC en boucle locale, cas nominal et cas d'attaque.
use ainonymous_quic::NodeIdentity;
use relay_quic_spike::{connect, echo_once, send_activations, Worker, WorkerEvent};
use std::time::Duration;

async fn worker() -> Worker {
    Worker::start("127.0.0.1:0".parse().unwrap(), NodeIdentity::generate())
        .await
        .unwrap()
}

/// Une session refusée : la connexion ou le premier échange échoue, et le
/// worker n'a jamais remis la session au handler.
async fn assert_refused(w: &mut Worker, id: &NodeIdentity, offer: ainonymous_quic::SessionOffer) {
    if let Ok((_ep, s)) = connect(id, offer).await {
        let r = tokio::time::timeout(Duration::from_secs(8), echo_once(&s, 64)).await;
        assert!(!matches!(r, Ok(Ok(_))), "l'échange aurait dû échouer");
    }
    let ev = tokio::time::timeout(Duration::from_millis(300), w.events.recv()).await;
    assert!(
        ev.is_err(),
        "aucune session ne doit être authentifiée : {ev:?}"
    );
}

#[tokio::test]
async fn session_nominale_echo_et_activations_integres() {
    let mut w = worker().await;
    let client = NodeIdentity::generate();
    let offer = w.offer_for(client.public_key_bytes(), Some((0, 13)), w.addr);
    let (_ep, s) = connect(&client, offer).await.unwrap();
    echo_once(&s, 64).await.unwrap();
    match w.events.recv().await.unwrap() {
        WorkerEvent::Authenticated {
            client_key,
            layer_range,
        } => {
            assert_eq!(client_key, client.public_key_bytes());
            assert_eq!(layer_range, Some((0, 13)));
        }
        e => panic!("inattendu {e:?}"),
    }
    send_activations(&s, 4 * 1024 * 1024, 14, 27).await.unwrap();
    match w.events.recv().await.unwrap() {
        WorkerEvent::Activations {
            bytes,
            layer_start,
            layer_end,
            ..
        } => {
            assert_eq!((bytes, layer_start, layer_end), (4 * 1024 * 1024, 14, 27));
        }
        e => panic!("inattendu {e:?}"),
    }
}

#[tokio::test]
async fn client_non_autorise_refuse_meme_avec_token_vole() {
    let mut w = worker().await;
    let legit = NodeIdentity::generate();
    let attacker = NodeIdentity::generate();
    let offer = w.offer_for(legit.public_key_bytes(), None, w.addr);
    assert_refused(&mut w, &attacker, offer).await;
}

#[tokio::test]
async fn rejeu_du_token_refuse() {
    let mut w = worker().await;
    let client = NodeIdentity::generate();
    let offer = w.offer_for(client.public_key_bytes(), None, w.addr);
    let (_ep, s) = connect(&client, offer.clone()).await.unwrap();
    echo_once(&s, 64).await.unwrap();
    assert!(matches!(
        w.events.recv().await,
        Some(WorkerEvent::Authenticated { .. })
    ));
    // Même client, même offre : le token est à usage unique.
    assert_refused(&mut w, &client, offer).await;
}

#[tokio::test]
async fn token_altere_refuse() {
    let mut w = worker().await;
    let client = NodeIdentity::generate();
    let mut offer = w.offer_for(client.public_key_bytes(), None, w.addr);
    offer.session_token[0] ^= 0xFF;
    assert_refused(&mut w, &client, offer).await;
}

#[tokio::test]
async fn faux_worker_refuse_par_le_client() {
    // Le client épingle la clé du worker attendu : un autre nœud (MITM) qui
    // répond à la même adresse est rejeté au handshake TLS.
    let mut w = worker().await;
    let client = NodeIdentity::generate();
    let mut offer = w.offer_for(client.public_key_bytes(), None, w.addr);
    offer.peer_pubkey = Some(NodeIdentity::generate().public_key_bytes());
    assert!(connect(&client, offer).await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(300), w.events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn offre_expiree_refusee() {
    let mut w = worker().await;
    let client = NodeIdentity::generate();
    let mut offer = w.offer_for(client.public_key_bytes(), None, w.addr);
    offer.expires_at_unix_ms = 1;
    assert!(connect(&client, offer).await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(300), w.events.recv())
            .await
            .is_err()
    );
}
