pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op {
        Op::StoreEntry(store_entry) => {
            if let EntryTypes::Warrant(warrant) = store_entry.action.app_entry() {
                // Vérifier que la signature a la bonne taille
                if warrant.signature.len() != 64 {
                    return Ok(ValidateCallbackResult::Invalid(
                        "Invalid signature length (must be 64 bytes)".to_string(),
                    ));
                }

                // Vérifier que l'issuer correspond à l'agent qui crée l'entrée
                let author = store_entry.action.hashed.content.author();
                if warrant.issuer != author.get_raw_32() {
                    return Ok(ValidateCallbackResult::Invalid(
                        "Issuer must match the agent creating the warrant".to_string(),
                    ));
                }

                Ok(ValidateCallbackResult::Valid)
            } else {
                Ok(ValidateCallbackResult::Valid)
            }
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}
