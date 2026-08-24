//! Print the effective Holochain DNA hash for packed DNA bundles.

use holochain_types::prelude::{DnaBundle, DnaModifiersOpt};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let paths = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        !paths.is_empty(),
        "usage: dna-hashes <bundle.dna> [bundle.dna ...]"
    );

    for path in paths {
        let bundle = DnaBundle::unpack(std::fs::File::open(&path)?)?;
        let (dna_file, _) = bundle.into_dna_file(DnaModifiersOpt::none()).await?;
        println!("{}\t{}", dna_file.dna_hash(), path.display());
    }

    Ok(())
}
