//! Local admission gate. Revocations are monotonic; snapshots never un-revoke.
use crate::QuicError;
pub use ainonymous_revocation::{Revocation, TransportBinding};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct RevocationGate(Arc<Mutex<State>>);
#[derive(Debug)]
struct State {
    network: String,
    admin: [u8; 32],
    local_key: [u8; 32],
    cache: PathBuf,
    refreshed: Option<Instant>,
    denied: HashMap<String, Revocation>,
    bindings: HashMap<[u8; 32], String>,
    active: HashMap<usize, ([u8; 32], quinn::Connection)>,
    next: usize,
}

fn rejected(reason: impl Into<String>) -> QuicError {
    QuicError::ConnectFailed(format!("CH6 admission denied: {}", reason.into()))
}
impl RevocationGate {
    pub fn new(
        network: String,
        admin: [u8; 32],
        local_key: [u8; 32],
        cache: PathBuf,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(!network.is_empty(), "CH6 network_id is required");
        let gate = Self(Arc::new(Mutex::new(State {
            network,
            admin,
            local_key,
            cache,
            refreshed: None,
            denied: HashMap::new(),
            bindings: HashMap::new(),
            active: HashMap::new(),
            next: 0,
        })));
        let path = gate.0.lock().unwrap().cache.clone();
        match std::fs::read(&path) {
            Ok(bytes) => {
                let records: Vec<Revocation> = serde_json::from_slice(&bytes)?;
                for r in records {
                    gate.apply(r)?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        Ok(gate)
    }
    pub fn apply(&self, r: Revocation) -> anyhow::Result<()> {
        let mut s = self.0.lock().unwrap();
        r.verify(&s.network, &s.admin).map_err(anyhow::Error::msg)?;
        if s.denied.contains_key(&r.agent_id) {
            return Ok(());
        }
        // Persist before applying/acknowledging, so a restart cannot restore access.
        let mut denied = s.denied.clone();
        denied.entry(r.agent_id.clone()).or_insert(r);
        let records: Vec<_> = denied.values().collect();
        let persist = (|| -> anyhow::Result<()> {
            let temp = s.cache.with_extension("tmp");
            let mut file = std::fs::File::create(&temp)?;
            use std::io::Write;
            file.write_all(&serde_json::to_vec(&records)?)?;
            file.sync_all()?;
            std::fs::rename(&temp, &s.cache)?;
            // Make the rename durable on Unix as well as the file contents.
            #[cfg(unix)]
            std::fs::File::open(
                s.cache
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| std::path::Path::new(".")),
            )?
            .sync_all()?;
            Ok(())
        })();
        if let Err(error) = persist {
            s.refreshed = None;
            s.enforce();
            return Err(error);
        }
        s.denied = denied;
        s.enforce();
        Ok(())
    }
    pub fn refresh(
        &self,
        records: Vec<Revocation>,
        bindings: Vec<TransportBinding>,
    ) -> anyhow::Result<()> {
        // Do not mark fresh on a partial/invalid snapshot.
        for r in records {
            self.apply(r)?;
        }
        let mut s = self.0.lock().unwrap();
        let mut next = s.bindings.clone();
        for b in bindings {
            let key: [u8; 32] = hex::decode(b.node_pubkey)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid transport key"))?;
            if let Some(existing) = next.get(&key) {
                anyhow::ensure!(existing == &b.agent_id, "ambiguous transport binding");
            }
            next.insert(key, b.agent_id);
        }
        s.bindings = next;
        s.refreshed = Some(Instant::now());
        s.enforce();
        Ok(())
    }
    pub fn check(&self, peer: &[u8; 32]) -> Result<(), QuicError> {
        self.0.lock().unwrap().check(peer)
    }
    /// Periodic watchdog also terminates sessions if refresh is stale.
    pub fn enforce(&self) {
        self.0.lock().unwrap().enforce();
    }
    /// Check and track under one lock: publication cannot race acceptance.
    pub fn track(
        &self,
        peer: [u8; 32],
        conn: quinn::Connection,
    ) -> Result<ActiveSession, QuicError> {
        let mut s = self.0.lock().unwrap();
        if let Err(e) = s.check(&peer) {
            conn.close(0xC6u32.into(), e.to_string().as_bytes());
            return Err(e);
        }
        let id = s.next;
        s.next += 1;
        s.active.insert(id, (peer, conn));
        Ok(ActiveSession {
            gate: self.clone(),
            id,
        })
    }
}
impl State {
    fn check(&self, peer: &[u8; 32]) -> Result<(), QuicError> {
        if self
            .refreshed
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(3))
        {
            return Err(rejected("revocation snapshot unavailable or stale"));
        }
        for key in [&self.local_key, peer] {
            let agent = self
                .bindings
                .get(key)
                .ok_or_else(|| rejected("unregistered transport key"))?;
            if let Some(r) = self.denied.get(agent) {
                return Err(rejected(format!("revoked {}: {}", agent, r.reason)));
            }
        }
        Ok(())
    }
    fn enforce(&mut self) {
        self.active
            .retain(|_, (_, conn)| conn.close_reason().is_none());
        for (peer, conn) in self.active.values() {
            if let Err(e) = self.check(peer) {
                conn.close(0xC6u32.into(), e.to_string().as_bytes());
                tracing::warn!(status = "failed", reason = %e, "CH6 session/job cancelled");
            }
        }
    }
}
/// Lifetime is owned by the session, not a detached task.
pub struct ActiveSession {
    gate: RevocationGate,
    id: usize,
}
impl Drop for ActiveSession {
    fn drop(&mut self) {
        if let Some((_, conn)) = self.gate.0.lock().unwrap().active.remove(&self.id) {
            conn.close(0u32.into(), b"session ended");
        }
    }
}
