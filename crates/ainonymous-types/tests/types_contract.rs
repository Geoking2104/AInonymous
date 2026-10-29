//! Contrats des types partagés.
//!
//! Ces tests verrouillent ce que le reste du workspace suppose vrai :
//! le format binaire des activations sur le fil, la vérification des warrants
//! signés, le scoring de nœuds et la compatibilité OpenAI des structures d'API.

use ainonymous_types::api::{ApiError, ChatCompletionChunk, ChatCompletionRequest};
use ainonymous_types::errors::AInonymousError;
use ainonymous_types::inference::{
    ActivationHeader, ChatMessage, DType, MessageContent, MessageRole,
};
use ainonymous_types::node::{
    AgentId, ComputeBackend, ExecutionPlan, GeoLocation, GpuVendor, LoadedModel, NodeCapabilities,
    NodeHeartbeat, NodeScore, PipelineStage, Quantization,
};
use ainonymous_types::warrants::{Warrant, WarrantType};
use ed25519_dalek::SigningKey;

// ─── Format binaire des activations (data plane) ─────────────────────────────

fn header_exemple() -> ActivationHeader {
    let mut request_id = [0u8; 36];
    request_id[..8].copy_from_slice(b"req-0001");
    ActivationHeader {
        request_id,
        layer_start: 7,
        layer_end: 19,
        seq_len: 4,
        hidden_size: 4096,
        dtype: DType::F16,
        compressed: true,
        speculative_k: 3,
    }
}

#[test]
fn entete_activation_est_bien_64_octets() {
    assert_eq!(ActivationHeader::SIZE, 64);
    assert_eq!(header_exemple().to_bytes().len(), 64);
}

#[test]
fn entete_activation_aller_retour() {
    let h = header_exemple();
    let byte = h.to_bytes();
    let relu = ActivationHeader::from_bytes(&byte);

    assert_eq!(relu.request_id, h.request_id);
    assert_eq!(relu.layer_start, 7);
    assert_eq!(relu.layer_end, 19);
    assert_eq!(relu.seq_len, 4);
    assert_eq!(relu.hidden_size, 4096);
    assert_eq!(relu.dtype, DType::F16);
    assert!(relu.compressed);
    assert_eq!(relu.speculative_k, 3);
}

#[test]
fn entete_activation_offsets_stables() {
    // Le layout est documenté (docs/PYTORCH_INTEGRATION.md et doc du type) :
    // tout décalage casserait l'interopérabilité des implémentations.
    let byte = header_exemple().to_bytes();

    assert_eq!(&byte[0..8], b"req-0001");
    assert_eq!(u32::from_le_bytes(byte[36..40].try_into().unwrap()), 7);
    assert_eq!(u32::from_le_bytes(byte[40..44].try_into().unwrap()), 19);
    assert_eq!(u32::from_le_bytes(byte[44..48].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(byte[48..52].try_into().unwrap()), 4096);
    assert_eq!(byte[52], 1, "dtype F16 = 1");
    assert_eq!(byte[53], 1, "compressed = 1");
    assert_eq!(byte[54], 3, "speculative_k");
}

#[test]
fn entete_activation_reserve_est_nul() {
    let byte = header_exemple().to_bytes();
    assert!(
        byte[55..].iter().all(|b| *b == 0),
        "les 9 octets réservés doivent rester à zéro"
    );
}

#[test]
fn dtype_encodage_stable() {
    assert_eq!(DType::F32 as u8, 0);
    assert_eq!(DType::F16 as u8, 1);
    assert_eq!(DType::Bf16 as u8, 2);
    assert_eq!(DType::F32.bytes_per_element(), 4);
    assert_eq!(DType::F16.bytes_per_element(), 2);
    assert_eq!(DType::Bf16.bytes_per_element(), 2);
    // Un identifiant inconnu retombe sur Bf16 (2 octets) plutôt que de paniquer.
    assert_eq!(DType::from(9), DType::Bf16);
}

#[test]
fn taille_tenseur_annoncee() {
    let h = header_exemple(); // 4 tokens × 4096 × 2 octets (F16)
    assert_eq!(h.tensor_size_bytes(), 4 * 4096 * 2);
}

// ─── Warrants signés (Ed25519ph + contexte) ─────────────────────────────────

fn cle(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

#[test]
fn warrant_signe_est_verifie() {
    let issuer = cle(7);
    let w = Warrant::new_signed(
        &issuer,
        WarrantType::ModelClaim,
        serde_json::json!({"model_id": "gemma-3-4b", "vram_required_gb": 3.5}),
        3600,
    )
    .expect("signature");

    assert!(w.verify(&issuer.verifying_key()));
    assert_eq!(w.issuer, issuer.verifying_key().to_bytes());
    assert!(!w.is_expired());
}

#[test]
fn warrant_altere_est_refuse() {
    let issuer = cle(7);
    let mut w = Warrant::new_signed(
        &issuer,
        WarrantType::NodeCapabilities,
        serde_json::json!({"vram_gb": 8.0}),
        3600,
    )
    .expect("signature");
    assert!(w.verify(&issuer.verifying_key()));

    // Altération du payload après signature : la vérification doit échouer.
    w.payload = serde_json::json!({"vram_gb": 80.0});
    assert!(!w.verify(&issuer.verifying_key()));
}

#[test]
fn warrant_autre_emetteur_est_refuse() {
    let issuer = cle(7);
    let imposteur = cle(8);
    let w = Warrant::new_signed(&issuer, WarrantType::ExecutionProof, serde_json::json!({}), 60)
        .expect("signature");

    assert!(!w.verify(&imposteur.verifying_key()));
}

#[test]
fn warrant_expire_est_refuse() {
    let issuer = cle(7);
    let mut w = Warrant::new_signed(&issuer, WarrantType::ModelClaim, serde_json::json!({}), 1)
        .expect("signature");
    // Émission dans le passé : TTL d'une seconde déjà écoulé.
    w.issued_at = 1;
    assert!(w.is_expired());
    assert!(!w.verify(&issuer.verifying_key()));
}

#[test]
fn warrant_sans_ttl_n_expire_pas() {
    let issuer = cle(7);
    let mut w = Warrant::new_signed(&issuer, WarrantType::ModelClaim, serde_json::json!({}), 0)
        .expect("signature");
    w.issued_at = 1;
    assert!(!w.is_expired());
}

#[test]
fn warrant_signature_malformee_ne_panique_pas() {
    let issuer = cle(7);
    let mut w = Warrant::new_signed(&issuer, WarrantType::ModelClaim, serde_json::json!({}), 60)
        .expect("signature");
    w.signature.truncate(10);
    assert!(!w.verify(&issuer.verifying_key()));
}

#[test]
fn type_de_warrant_est_stable_dans_la_signature() {
    // Le type fait partie du message signé (voir `new_signed`) : le changer
    // après coup invalide la signature.
    let issuer = cle(7);
    let mut w = Warrant::new_signed(&issuer, WarrantType::ModelClaim, serde_json::json!({}), 60)
        .expect("signature");
    w.warrant_type = WarrantType::ExecutionProof;
    assert!(!w.verify(&issuer.verifying_key()));
}

// ─── Scoring de nœuds ───────────────────────────────────────────────────────

fn capacites(vram_gb: f32, region: Option<&str>) -> NodeCapabilities {
    NodeCapabilities {
        agent_id: "agent-a".to_string(),
        vram_gb,
        ram_gb: 32.0,
        gpu_vendor: GpuVendor::Nvidia {
            vram_gb,
            compute_capability: "8.6".to_string(),
        },
        compute_backends: vec![ComputeBackend::Cuda, ComputeBackend::Cpu],
        loaded_models: vec!["gemma-3-4b".to_string()],
        max_concurrent_requests: 4,
        network_bandwidth_mbps: Some(1000.0),
        region_hint: region.map(|r| r.to_string()),
        quic_endpoint: Some("127.0.0.1:9000".to_string()),
        node_pubkey: None,
        geo_location: Some(GeoLocation {
            latitude: 48.8,
            longitude: 2.3,
            accuracy_km: Some(10.0),
        }),
    }
}

fn heartbeat(load: f32, slots: u8) -> NodeHeartbeat {
    NodeHeartbeat {
        agent_id: "agent-a".to_string(),
        current_load: load,
        available_slots: slots,
        queue_depth: 0,
        memory_pressure: 0.1,
        temperature_c: Some(45.0),
        timestamp_ms: 1_700_000_000_000,
    }
}

#[test]
fn score_prefere_un_noeud_inactif() {
    let caps = capacites(0.0, None);
    let idle = NodeScore::compute(&caps, &heartbeat(0.0, 8), None);
    let sature = NodeScore::compute(&caps, &heartbeat(1.0, 0), None);
    assert!(idle > sature, "idle={idle} devrait dépasser sature={sature}");
}

#[test]
fn score_bonus_de_region() {
    let caps = capacites(0.0, Some("eu-west-3"));
    let hb = heartbeat(0.5, 4);
    let avec = NodeScore::compute(&caps, &hb, Some("eu-"));
    let sans = NodeScore::compute(&caps, &hb, Some("us-"));
    assert!((avec - sans - 10.0).abs() < f32::EPSILON);
}

#[test]
fn score_vram_est_plafonne() {
    let hb = heartbeat(0.5, 4);
    let gros = NodeScore::compute(&capacites(200.0, None), &hb, None);
    let plafond = NodeScore::compute(&capacites(80.0, None), &hb, None);
    assert!((gros - plafond).abs() < f32::EPSILON);
}

#[test]
fn vram_du_vendeur_gpu() {
    assert_eq!(GpuVendor::CpuOnly.vram_gb(), 0.0);
    assert_eq!(GpuVendor::Amd { vram_gb: 16.0 }.vram_gb(), 16.0);
    assert_eq!(GpuVendor::Intel { vram_gb: 2.0 }.vram_gb(), 2.0);
}

// ─── Sérialisation des plans et quantifications ─────────────────────────────

#[test]
fn plan_solo_est_tague_mode() {
    let plan = ExecutionPlan::Solo {
        node: AgentId::from("agent-a"),
        quic_endpoint: "127.0.0.1:9000".parse().unwrap(),
    };
    let v = serde_json::to_value(&plan).unwrap();
    assert_eq!(v["mode"], "solo");
    assert_eq!(v["node"], "agent-a");
    assert_eq!(v["quic_endpoint"], "127.0.0.1:9000");

    let relu: ExecutionPlan = serde_json::from_value(v).unwrap();
    assert!(matches!(relu, ExecutionPlan::Solo { .. }));
}

#[test]
fn plan_pipeline_aller_retour() {
    let plan = ExecutionPlan::PipelineSplit {
        stages: vec![PipelineStage {
            node: AgentId::from("agent-a"),
            quic_endpoint: "127.0.0.1:9000".parse().unwrap(),
            layer_start: 0,
            layer_end: 15,
            is_last: false,
        }],
    };
    let texte = serde_json::to_string(&plan).unwrap();
    assert!(texte.contains("\"mode\":\"pipeline_split\""));
    let relu: ExecutionPlan = serde_json::from_str(&texte).unwrap();
    match relu {
        ExecutionPlan::PipelineSplit { stages } => {
            assert_eq!(stages.len(), 1);
            assert_eq!((stages[0].layer_start, stages[0].layer_end), (0, 15));
        }
        autre => panic!("plan inattendu: {autre:?}"),
    }
}

#[test]
fn quantification_connue_et_inconnue() {
    assert_eq!(
        serde_json::to_string(&Quantization::Q4KM).unwrap(),
        "\"Q4KM\""
    );
    let connu: Quantization = serde_json::from_str("\"F16\"").unwrap();
    assert_eq!(connu, Quantization::F16);

    // Toute valeur non reconnue tombe dans `Other` sans erreur.
    let autre: Quantization = serde_json::from_str("\"Q4_K_M\"").unwrap();
    assert_eq!(autre, Quantization::Other("Q4_K_M".to_string()));
}

#[test]
fn modele_charge_aller_retour() {
    let m = LoadedModel {
        model_id: "gemma-3-4b".to_string(),
        model_hash: "a".repeat(64),
        quantization: Quantization::Q4KM,
        layer_range: Some((0, 21)),
        expert_ids: None,
        context_size: 8192,
        ready: true,
        tokens_per_second_avg: Some(42.0),
    };
    let texte = serde_json::to_string(&m).unwrap();
    let relu: LoadedModel = serde_json::from_str(&texte).unwrap();
    assert_eq!(relu.model_id, m.model_id);
    assert_eq!(relu.layer_range, Some((0, 21)));
    assert!(relu.ready);
}

// ─── Compatibilité OpenAI ───────────────────────────────────────────────────

#[test]
fn requete_chat_minimale_prend_les_defauts() {
    let json = r#"{"model":"gemma-3-4b","messages":[{"role":"user","content":"bonjour"}]}"#;
    let req: ChatCompletionRequest = serde_json::from_str(json).expect("requête OpenAI valide");

    assert_eq!(req.model, "gemma-3-4b");
    assert_eq!(req.max_tokens, 2048);
    assert!((req.temperature - 0.7).abs() < f32::EPSILON);
    assert!((req.top_p - 0.9).abs() < f32::EPSILON);
    assert!(!req.stream);
    assert!(req.ainonymous.is_none());
    assert_eq!(req.messages[0].role, MessageRole::User);
    assert!(matches!(&req.messages[0].content, MessageContent::Text(t) if t == "bonjour"));
}

#[test]
fn contenu_multimodal_est_detecte() {
    let json = r#"{"role":"user","content":[{"type":"text","text":"décris"},{"type":"image_url","image_url":{"url":"https://exemple.test/a.png"}}]}"#;
    let msg: ChatMessage = serde_json::from_str(json).expect("message multimodal");
    match msg.content {
        MessageContent::Parts(parts) => assert_eq!(parts.len(), 2),
        autre => panic!("attendu Parts, obtenu {autre:?}"),
    }
}

#[test]
fn chunks_de_streaming_ont_la_forme_openai() {
    let premier = serde_json::to_value(ChatCompletionChunk::first("chatcmpl-1", "gemma")).unwrap();
    assert_eq!(premier["object"], "chat.completion.chunk");
    assert_eq!(premier["choices"][0]["delta"]["role"], "assistant");
    assert!(premier["choices"][0]["delta"].get("content").is_none());

    let token = serde_json::to_value(ChatCompletionChunk::token("chatcmpl-1", "gemma", "bon"))
        .unwrap();
    assert_eq!(token["choices"][0]["delta"]["content"], "bon");
    assert!(token["choices"][0]["delta"].get("role").is_none());

    let fin = serde_json::to_value(ChatCompletionChunk::done("chatcmpl-1", "gemma")).unwrap();
    assert_eq!(fin["choices"][0]["finish_reason"], "stop");
    assert_eq!(fin["choices"][0]["delta"], serde_json::json!({}));
}

#[test]
fn erreur_api_no_node_porte_le_detail() {
    let err = ApiError::no_node("gemma-3-27b", 8.0, Some("gemma-3-4b"));
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(v["error"]["code"], "NO_CAPABLE_NODE");
    assert_eq!(v["error"]["type"], "mesh_unavailable");
    assert_eq!(v["error"]["ainonymous"]["max_available_vram_gb"], 8.0);
}

#[test]
fn statut_http_des_erreurs() {
    assert_eq!(AInonymousError::UnknownModel { model: "x".into() }.http_status(), 404);
    assert_eq!(AInonymousError::MeshOverloaded { retry_after_seconds: 5 }.http_status(), 429);
    assert_eq!(AInonymousError::BadRequest { reason: "x".into() }.http_status(), 400);
    assert_eq!(
        AInonymousError::NoCapableNode { model: "x".into() }.http_status(),
        503
    );
    assert_eq!(AInonymousError::Internal("boom".into()).http_status(), 500);
}
