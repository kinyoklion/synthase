use clap::{Parser, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "synthase")]
#[command(about = "Automated release management using conventional commits")]
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

    /// Output release information for tags/releases (JSON output)
    Release,

    /// Initialize synthase configuration for a repository
    Bootstrap {
        /// Release type (e.g., "rust", "node", "simple", "python", "go")
        #[arg(long, default_value = "simple")]
        release_type: String,

        /// Initial version
        #[arg(long, default_value = "0.0.0")]
        initial_version: String,

        /// Package name / component
        #[arg(long)]
        component: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::ReleasePr => cmd_release_pr(&cli),
        Commands::Release => cmd_release(&cli),
        Commands::Bootstrap {
            release_type,
            initial_version,
            component,
        } => cmd_bootstrap(&cli, release_type, initial_version, component.as_deref()),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn cmd_release_pr(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let repo_path = PathBuf::from(&cli.repo_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&cli.repo_path));

    let output = synthase::manifest::process_repo(&repo_path)?;

    if output.releases.is_empty() {
        eprintln!("No releases to create.");
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "releases": [],
                "pull_requests": []
            }))?
        );
        return Ok(());
    }

    let config_path = repo_path.join("synthase-config.json");
    let config = synthase::config::load_config(&config_path)?;

    let pr_title =
        synthase::manifest::format_pr_title(&output.releases, &config, &cli.target_branch);
    let pr_body = synthase::manifest::format_pr_body(&output.releases, &config);

    let releases_json = build_releases_json(&output.releases);
    let all_files = build_files_json(&output);

    let result = json!({
        "releases": releases_json,
        "pull_requests": [{
            "title": pr_title,
            "body": pr_body,
            "branch": format!("synthase--branches--{}", cli.target_branch),
            "files": all_files,
        }],
    });

    if cli.dry_run {
        eprintln!("Dry run — no files written.");
    } else {
        apply_file_updates(&repo_path, &output)?;
        eprintln!(
            "Applied {} release(s) with {} file update(s).",
            output.releases.len(),
            all_files.len()
        );
    }

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn cmd_release(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let repo_path = PathBuf::from(&cli.repo_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&cli.repo_path));

    let output = synthase::manifest::process_repo(&repo_path)?;

    if output.releases.is_empty() {
        eprintln!("No releases found.");
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "releases": [] }))?
        );
        return Ok(());
    }

    // Output release information (tags, versions, notes) for creating GitHub releases
    let releases_json: Vec<serde_json::Value> = output
        .releases
        .iter()
        .map(|r| {
            json!({
                "component": r.component,
                "path": r.package_path,
                "version": r.new_version.to_string(),
                "tag": r.tag,
                "release_notes": r.changelog_entry,
                "draft": r.config.draft,
                "prerelease": r.config.prerelease,
                "skip_github_release": r.config.skip_github_release,
            })
        })
        .collect();

    let result = json!({ "releases": releases_json });
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn cmd_bootstrap(
    cli: &Cli,
    release_type: &str,
    initial_version: &str,
    component: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let repo_path = PathBuf::from(&cli.repo_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&cli.repo_path));

    let config_path = repo_path.join("synthase-config.json");
    let manifest_path = repo_path.join(".synthase-manifest.json");

    if config_path.exists() {
        eprintln!("Config file already exists: {}", config_path.display());
        return Ok(());
    }

    // Build config
    let mut pkg_config = serde_json::Map::new();
    if let Some(comp) = component {
        pkg_config.insert("component".to_string(), json!(comp));
    }

    let config = json!({
        "release-type": release_type,
        "packages": {
            ".": pkg_config,
        }
    });

    let manifest = json!({
        ".": initial_version,
    });

    if cli.dry_run {
        eprintln!("Dry run — would create:");
        eprintln!("  {}", config_path.display());
        eprintln!("  {}", manifest_path.display());
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "config": config,
                "manifest": manifest,
            }))?
        );
    } else {
        std::fs::write(&config_path, serde_json::to_string_pretty(&config)? + "\n")?;
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest)? + "\n",
        )?;
        eprintln!("Created {}", config_path.display());
        eprintln!("Created {}", manifest_path.display());
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "config": config,
                "manifest": manifest,
            }))?
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn build_releases_json(
    releases: &[synthase::manifest::ComponentRelease],
) -> Vec<serde_json::Value> {
    releases
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
        .collect()
}

fn build_files_json(output: &synthase::manifest::ReleaseOutput) -> Vec<serde_json::Value> {
    let mut files = Vec::new();
    for release in &output.releases {
        for update in &release.file_updates {
            files.push(json!({
                "path": update.path,
                "content": update.content,
                "create_if_missing": update.create_if_missing,
            }));
        }
    }
    if let Some(ref mu) = output.manifest_update {
        files.push(json!({
            "path": mu.path,
            "content": mu.content,
            "create_if_missing": mu.create_if_missing,
        }));
    }
    files
}

fn apply_file_updates(
    repo_path: &Path,
    output: &synthase::manifest::ReleaseOutput,
) -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}
