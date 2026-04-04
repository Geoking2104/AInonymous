use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

mod commands;

#[derive(Parser)]
#[command(
    name = "ainonymous",
    about = "AInonymous — Inference LLM décentralisée sur Holochain",
    version,
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Rejoindre le mesh public automatiquement
    #[arg(long, short = 'a')]
    auto: bool,

    /// Démarrer avec un modèle spécifique
    #[arg(long, short = 'm')]
    model: Option<String>,

    /// Port du proxy API local (défaut: 9337)
    #[arg(long, default_value = "9337")]
    port: u16,

    /// Verbose
    #[arg(long, short = 'v', action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Commands {
    /// Démarrer le daemon AInonymous
    Start {
        /// Port du proxy (défaut: 9337)
        #[arg(long, default_value = "9337")]
        port: u16,
    },
    /// Arrêter le daemon AInonymous
    Stop,
    /// Afficher le statut du mesh
    Status,
    /// Lancer Goose avec le mesh AInonymous
    Goose {
        /// Profil de modèle (fast/standard/powerful)
        #[arg(long, default_value = "standard")]
        profile: String,
        /// Mode équipe multi-agents
        #[arg(long)]
        team: bool,
        /// Nombre d'agents en mode équipe
        #[arg(long, default_value = "3")]
        agents: u8,
    },
    /// Lancer le serveur MCP pour Goose
    Mcp {
        /// DNA Holochain cible
        #[arg(long)]
        dna: Option<String>,
    },
    /// Gestion des modèles
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// Blackboard partagé des agents
    Blackboard {
        /// Message à poster (format: "PREFIX: contenu")
        message: Option<String>,
        /// Rechercher dans le blackboard
        #[arg(long, short = 's')]
        search: Option<String>,
        /// Installer la compétence Blackboard pour Goose
        #[arg(long)]
        install_skill: bool,
        /// Serveur MCP pour le blackboard
        #[arg(long)]
        mcp: bool,
    },
    /// Afficher les nœuds du mesh
    Nodes {
        /// Filtrer par modèle
        #[arg(long, short = 'm')]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum ModelAction {
    /// Lister les modèles disponibles
    List,
    /// Télécharger un modèle
    Pull {
        model_id: String,
        #[arg(long, default_value = "q4_k_m")]
        quant: String,
    },
    /// Supprimer un modèle local
    Remove { model_id: String },
    /// Afficher les infos d'un modèle
    Info { model_id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive(format!("ainonymous={}", log_level).parse()?))
        .without_time()
        .init();

    let api_url = format!("http://127.0.0.1:{}/v1", cli.port);

    // Cas --auto : démarrer et rejoindre le mesh
    if cli.auto {
        return commands::start::run_auto(cli.model.as_deref(), cli.port).await;
    }

    // Cas --model sans subcommand : démarrer avec ce modèle
    if cli.model.is_some() && cli.command.is_none() {
        return commands::start::run_with_model(cli.model.as_deref().unwrap(), cli.port).await;
    }

    match cli.command.unwrap_or(Commands::Status) {
        Commands::Start { port } => {
            commands::start::run(port).await
        }
        Commands::Stop => {
            commands::control::stop(&api_url).await
        }
        Commands::Status => {
            commands::status::show(&api_url).await
        }
        Commands::Goose { profile, team, agents } => {
            commands::goose::launch(&api_url, &profile, team, agents).await
        }
        Commands::Mcp { dna } => {
            commands::mcp::start(dna.as_deref()).await
        }
        Commands::Model { action } => {
            commands::model::handle(action, &api_url).await
        }
        Commands::Blackboard { message, search, install_skill, mcp } => {
            commands::blackboard::handle(&api_url, message.as_deref(), search.as_deref(),
                install_skill, mcp).await
        }
        Commands::Nodes { model } => {
            commands::nodes::list(&api_url, model.as_deref()).await
        }
    }
}
