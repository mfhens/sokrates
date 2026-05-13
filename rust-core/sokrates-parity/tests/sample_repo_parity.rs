use anyhow::Result;
use sokrates_cli::analyze_from_cli_options;
use sokrates_parity::{load_java_golden, load_java_golden_inventory};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("sample-repo")
}

#[test]
fn sample_repo_matches_java_golden_file_inventory() -> Result<()> {
    let fixture = fixture_root();
    let input = fixture.join("input");
    let config = input.join("_sokrates").join("config.json");
    let goldens = fixture.join("goldens");
    let golden = goldens.join("analysisResults.json");

    let analysis = analyze_from_cli_options(None, Some(config))?;
    let java = load_java_golden(&golden)?;
    let java_inventory = load_java_golden_inventory(&goldens)?;

    let rust_paths = analysis
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();

    assert_eq!(analysis.metadata.name, java.display_name());
    assert_eq!(analysis.summary.total_files, java_inventory.len());
    assert_eq!(rust_paths, java_inventory);

    Ok(())
}
