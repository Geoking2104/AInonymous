//! Print the effective Holochain DNA hash for packed DNA bundles.

use holochain_types::prelude::{DnaBundle, DnaModifiersOpt};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let registry_path = match args.next() {
        Some(flag) if flag == "--check" => args.next().map(PathBuf::from),
        Some(path) => {
            let mut paths = vec![PathBuf::from(path)];
            paths.extend(args.map(PathBuf::from));
            return print_hashes(paths, None).await;
        }
        None => None,
    };
    let paths = args.map(PathBuf::from).collect::<Vec<_>>();
    print_hashes(paths, registry_path).await
}

async fn print_hashes(paths: Vec<PathBuf>, registry_path: Option<PathBuf>) -> anyhow::Result<()> {
    anyhow::ensure!(
        !paths.is_empty(),
        "usage: dna-hashes [--check registry.json] <bundle.dna> [bundle.dna ...]"
    );

    let registry = registry_path
        .as_ref()
        .map(std::fs::read_to_string)
        .transpose()?
        .map(|json| serde_json::from_str::<serde_json::Value>(&json))
        .transpose()?;

    for path in paths {
        let bundle = DnaBundle::unpack(std::fs::File::open(&path)?)?;
        let (dna_file, _) = bundle.into_dna_file(DnaModifiersOpt::none()).await?;
        let actual = dna_file.dna_hash().to_string();
        let normalized_path = path.to_string_lossy().replace('\\', "/");

        if let Some(registry) = &registry {
            let expected = registry
                .get("dnas")
                .and_then(|dnas| dnas.get(&normalized_path))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("no registered hash for {normalized_path}"))?;
            anyhow::ensure!(
                actual == expected,
                "DNA hash mismatch for {normalized_path}: expected {expected}, found {actual}"
            );
        }

        println!("{actual}\t{normalized_path}");
    }

    Ok(())
}
