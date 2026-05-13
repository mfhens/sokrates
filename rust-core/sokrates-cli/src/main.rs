use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(author, version, about = "Rust bootstrap CLI for Sokrates analysis")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Analyze {
        #[arg(long)]
        src_root: Option<PathBuf>,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    ExportData {
        #[arg(long)]
        src_root: Option<PathBuf>,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Analyze {
            src_root,
            config,
            output,
        } => {
            let analysis = sokrates_cli::analyze_from_cli_options(src_root, config)?;
            let json = serde_json::to_string_pretty(&analysis).context("serialize analysis")?;

            if let Some(path) = output {
                fs::write(&path, json)
                    .with_context(|| format!("write analysis to {}", path.display()))?;
            } else {
                println!("{json}");
            }
        }
        Command::ExportData {
            src_root,
            config,
            output_dir,
        } => {
            sokrates_cli::export_compat_data_from_cli_options(src_root, config, output_dir)?;
        }
    }

    Ok(())
}
