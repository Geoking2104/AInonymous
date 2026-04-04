use anyhow::Result;

pub async fn start(dna: Option<&str>) -> Result<()> {
    // Déléguer au binaire ainonymous-mcp
    let mcp_bin = std::env::current_exe()?
        .parent().unwrap()
        .join("ainonymous-mcp");

    let mut cmd = std::process::Command::new(&mcp_bin);
    if let Some(d) = dna { cmd.args(["--dna", d]); }

    let status = cmd.status()
        .map_err(|e| anyhow::anyhow!("MCP server non trouvé: {}", e))?;

    if !status.success() {
        anyhow::bail!("MCP server terminé avec erreur");
    }
    Ok(())
}
