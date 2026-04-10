use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rustlease-please")]
#[command(about = "Automated release management — a Rust reimplementation of release-please")]
#[command(version)]
struct Cli {
    /// Path to the git repository (defaults to current directory)
    #[arg(long, default_value = ".")]
    repo_path: String,

    /// Path to release-please-config.json
    #[arg(long, default_value = "release-please-config.json")]
    config_file: String,

    /// Path to .release-please-manifest.json
    #[arg(long, default_value = ".release-please-manifest.json")]
    manifest_file: String,

    /// Target branch
    #[arg(long, default_value = "main")]
    target_branch: String,

    /// Compute changes without writing any files
    #[arg(long)]
    dry_run: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compute the next release and output PR information
    ReleasePr,

    /// Output release information for a merged release PR
    Release,

    /// Initialize release-please configuration for a repository
    Bootstrap,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::ReleasePr => {
            eprintln!("release-pr: not yet implemented");
            std::process::exit(1);
        }
        Commands::Release => {
            eprintln!("release: not yet implemented");
            std::process::exit(1);
        }
        Commands::Bootstrap => {
            eprintln!("bootstrap: not yet implemented");
            std::process::exit(1);
        }
    }
}
