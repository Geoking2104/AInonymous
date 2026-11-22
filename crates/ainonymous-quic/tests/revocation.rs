use ainonymous_quic::revocation::{Revocation, RevocationGate, TransportBinding};
use ainonymous_quic::{
    create_endpoint, NodeIdentity, QuicListener, QuicSession, SessionConfig, SessionOffer,
};
use ed25519_dalek::{Signer, SigningKey};
use std::time::{Duration, Instant};

fn signed(admin: &SigningKey, agent: &str) -> Revocation {
    let mut r = Revocation {
        network_id: "ch6-test".into(),
        agent_id: agent.into(),
        scope: "mesh".into(),
        reason: "membership removed".into(),
        issued_at_ms: 42,
        signature: vec![],
    };
    r.signature = admin.sign(&r.signing_bytes()).to_bytes().to_vec();
    r
}
fn binding(agent: &str, id: &NodeIdentity) -> TransportBinding {
    TransportBinding {
        agent_id: agent.into(),
        node_pubkey: id.public_key_hex(),
    }
}
fn gate(
    dir: &std::path::Path,
    name: &str,
    local: &NodeIdentity,
    admin: &SigningKey,
) -> RevocationGate {
    RevocationGate::new(
        "ch6-test".into(),
        admin.verifying_key().to_bytes(),
        local.public_key_bytes(),
        dir.join(name),
    )
    .unwrap()
}

#[tokio::test]
async fn signed_removal_closes_both_sides_cancels_job_and_rejects_old_offer() {
    let dir = tempfile::tempdir().unwrap();
    let admin = SigningKey::from_bytes(&[9; 32]);
    let server = NodeIdentity::generate();
    let client = NodeIdentity::generate();
    let worker_gate = gate(dir.path(), "worker.json", &server, &admin);
    let client_gate = gate(dir.path(), "client.json", &client, &admin);
    let bindings = vec![binding("worker", &server), binding("client", &client)];
    worker_gate.refresh(vec![], bindings.clone()).unwrap();
    client_gate.refresh(vec![], bindings.clone()).unwrap();
    let mut listener = QuicListener::new("127.0.0.1:0".parse().unwrap(), &server)
        .await
        .unwrap();
    listener.set_revocations(worker_gate.clone());
    let mut offer = SessionOffer::new(listener.local_addr().unwrap(), None);
    offer.peer_pubkey = Some(server.public_key_bytes());
    offer.client_pubkey = Some(client.public_key_bytes());
    listener.register_session(offer.clone());
    let registry = listener.registry();
    let (started_tx, mut started_rx) = tokio::sync::mpsc::channel(1);
    let (stopped_tx, mut stopped_rx) = tokio::sync::mpsc::channel(1);
    let job_gate = worker_gate.clone();
    let client_key = client.public_key_bytes();
    let task = tokio::spawn(async move {
        listener
            .run(move |conn, _| {
                let started = started_tx.clone();
                let stopped = stopped_tx.clone();
                let gate = job_gate.clone();
                async move {
                    started.send(()).await.unwrap();
                    // Models the daemon's close-vs-work select, using a never-ending job.
                    let reason = tokio::select! {
                        r = conn.closed() => r.to_string(),
                        _ = std::future::pending::<()>() => unreachable!(),
                    };
                    let reason = gate
                        .check(&client_key)
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or(reason);
                    stopped.send(reason).await.unwrap();
                }
            })
            .await;
    });
    let ep = create_endpoint(Some("127.0.0.1:0".parse().unwrap()), &client)
        .await
        .unwrap();
    let config = SessionConfig {
        revocations: Some(client_gate.clone()),
        ..Default::default()
    };
    let session = QuicSession::connect(&ep, offer.clone(), config.clone(), &client)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), started_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let published = Instant::now();
    // Deliver only to worker: QUIC CLOSE must reach the other side without a second publication.
    worker_gate.apply(signed(&admin, "client")).unwrap();
    let worker_reason = tokio::time::timeout(Duration::from_secs(2), stopped_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let remote_reason = tokio::time::timeout(Duration::from_secs(2), session.connection.closed())
        .await
        .unwrap();
    assert!(worker_reason.contains("membership removed"));
    assert!(remote_reason.to_string().contains("membership removed"));
    assert!(published.elapsed() < Duration::from_secs(5));
    eprintln!(
        "CH6 loopback apply -> both sides closed/job future stopped: {:?}",
        published.elapsed()
    );
    // A peer unaware of the revocation gets an explicit server-side refusal
    // even with a fresh, otherwise valid token and the same old credentials.
    let mut new_offer = offer.clone();
    new_offer.session_token = vec![23; 32];
    registry.register(new_offer.clone());
    let refused = QuicSession::connect(&ep, new_offer, config.clone(), &client)
        .await
        .unwrap();
    let reason = tokio::time::timeout(Duration::from_secs(2), refused.connection.closed())
        .await
        .unwrap();
    assert!(reason.to_string().contains("revoked client"));
    assert!(
        started_rx.try_recv().is_err(),
        "revoked peer reached application handler"
    );
    // Refresh cannot erase a revocation; a newly advertised transport key stays denied.
    let rotated = NodeIdentity::generate();
    let mut next = bindings.clone();
    next.push(binding("client", &rotated));
    worker_gate.refresh(vec![], next).unwrap();
    assert!(worker_gate
        .check(&rotated.public_key_bytes())
        .unwrap_err()
        .to_string()
        .contains("revoked client"));
    client_gate
        .refresh(vec![signed(&admin, "client")], bindings.clone())
        .unwrap();
    let err = QuicSession::connect(&ep, offer, config, &client)
        .await
        .err()
        .unwrap();
    assert!(err.to_string().contains("revoked client"));
    // Restart plus old valid proof/session offer never clears the deny list.
    let restored = gate(dir.path(), "worker.json", &server, &admin);
    assert!(restored.check(&client.public_key_bytes()).is_err()); // not fresh yet
    restored.refresh(vec![], bindings).unwrap();
    assert!(restored
        .check(&client.public_key_bytes())
        .unwrap_err()
        .to_string()
        .contains("revoked client"));
    task.abort();
}

#[test]
fn invalid_signatures_unknown_keys_persistence_failure_and_stale_snapshot_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let admin = SigningKey::from_bytes(&[9; 32]);
    let a = NodeIdentity::generate();
    let b = NodeIdentity::generate();
    let g = gate(dir.path(), "state.json", &a, &admin);
    assert!(g.check(&b.public_key_bytes()).is_err());
    g.refresh(vec![], vec![binding("a", &a), binding("b", &b)])
        .unwrap();
    assert!(g.check(&b.public_key_bytes()).is_ok());
    assert!(g.check(&[0; 32]).is_err());
    let mut forged = signed(&admin, "b");
    forged.reason = "forged".into();
    assert!(g.apply(forged).is_err());
    assert!(g
        .apply(signed(&SigningKey::from_bytes(&[8; 32]), "b"))
        .is_err());
    assert!(g.check(&b.public_key_bytes()).is_ok());
    std::thread::sleep(Duration::from_millis(3050));
    assert!(g
        .check(&b.public_key_bytes())
        .unwrap_err()
        .to_string()
        .contains("stale"));
    let bad = RevocationGate::new(
        "ch6-test".into(),
        admin.verifying_key().to_bytes(),
        a.public_key_bytes(),
        dir.path().join("missing/state.json"),
    )
    .unwrap();
    assert!(bad.apply(signed(&admin, "b")).is_err());
}
