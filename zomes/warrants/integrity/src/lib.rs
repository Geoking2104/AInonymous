use hdi::prelude::*;
use serde::{Deserialize, Serialize};

#[hdk_entry_helper]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Warrant {
    pub issuer: [u8; 32],
    pub warrant_type: WarrantType,
    pub payload: serde_json::Value,
    pub signature: Vec<u8>,
    pub issued_at: u64,
    pub ttl_seconds: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WarrantType {
    ModelClaim,
    NodeCapabilities,
    ExecutionProof,
    Custom(String),
}

#[hdk_entry_defs]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    #[entry_def(required_validations = 5)]
    Warrant(Warrant),
}

/// Link types pour relier les warrants aux agents
#[hdk_link_types]
pub enum LinkTypes {
    AgentToWarrants,
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op {
        Op::StoreEntry(store_entry) => {
            if let EntryTypes::Warrant(warrant) = store_entry.action.app_entry() {
                if warrant.signature.len() != 64 {
                    return Ok(ValidateCallbackResult::Invalid("Invalid signature length".to_string()));
                }
                Ok(ValidateCallbackResult::Valid)
            } else {
                Ok(ValidateCallbackResult::Valid)
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}
