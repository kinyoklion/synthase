use clap::{Parser, Subcommand};
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rustlease-please")]
#[command(about = "Automated release management — a Rust reimplementation of release-please")]
#[command(version)]
struct Cli {
    /// Path to the git repository (defaults to current directory)
    #[arg(long, default_value = ".")]
    repo_path: String,

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
    /// Compute the next release and output PR information as JSON
    ReleasePr,

    /// Output release information for a merged release PR
    Release,

    /// Initialize release-please configuration for a repository
    Bootstrap,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::ReleasePr => cmd_release_pr(&cli),
        Commands::Release => {
            eprintln!("release: not yet implemented");
            std::process::exit(1);
        }
        Commands::Bootstrap => {
            eprintln!("bootstrap: not yet implemented");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn cmd_release_pr(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let repo_path = PathBuf::from(&cli.repo_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&cli.repo_path));

    let output = rustlease_please::manifest::process_repo(&repo_path)?;

    if output.releases.is_empty() {
        eprintln!("No releases to create.");
        // Output empty JSON for machine consumption
        println!("{}", serde_json::to_string_pretty(&json!({
            "releases": [],
            "pull_requests": []
        }))?);
        return Ok(());
    }

    // Load config for PR formatting
    let config_path = repo_path.join("release-please-config.json");
    let config = rustlease_please::config::load_config(&config_path)?;

    let pr_title = rustlease_please::manifest::format_pr_title(
        &output.releases,
        &config,
        &cli.target_branch,
    );
    let pr_body = rustlease_please::manifest::format_pr_body(&output.releases, &config);

    // Build JSON output
    let releases_json: Vec<serde_json::Value> = output
        .releases
        .iter()
        .map(|r| {
            json!({
                "component": r.component,
                "path": r.package_path,
                "current_version": r.current_version.as_ref().map(|v| v.to_string()),
                "new_version": r.new_version.to_string(),
                "tag": r.tag,
                "changelog_entry": r.changelog_entry,
                "draft": r.config.draft,
                "prerelease": r.config.prerelease,
                "skip_github_release": r.config.skip_github_release,
            })
        })
        .collect();

    let mut all_files: Vec<serde_json::Value> = Vec::new();
    for release in &output.releases {
        for update in &release.file_updates {
            all_files.push(json!({
                "path": update.path,
                "content": update.content,
                "create_if_missing": update.create_if_missing,
            }));
        }
    }
    if let Some(ref mu) = output.manifest_update {
        all_files.push(json!({
            "path": mu.path,
            "content": mu.content,
            "create_if_missing": mu.create_if_missing,
        }));
    }

    let result = json!({
        "releases": releases_json,
        "pull_requests": [{
            "title": pr_title,
            "body": pr_body,
            "branch": format!("release-please--branches--{}", cli.target_branch),
            "files": all_files,
        }],
    });

    if cli.dry_run {
        eprintln!("Dry run — no files written.");
    } else {
        // Apply file updates to disk
        for release in &output.releases {
            for update in &release.file_updates {
                let full_path = repo_path.join(&update.path);
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if update.create_if_missing || full_path.exists() {
                    std::fs::write(&full_path, &update.content)?;
                }
            }
        }
        if let Some(ref mu) = output.manifest_update {
            std::fs::write(repo_path.join(&mu.path), &mu.content)?;
        }
        eprintln!(
            "Applied {} release(s) with {} file update(s).",
            output.releases.len(),
            all_files.len()
        );
    }

    // Always output JSON to stdout
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
