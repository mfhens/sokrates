use anyhow::Result;
use sokrates_cli::analyze_from_cli_options;
use sokrates_ir::FileRecord;
use sokrates_parity::{JavaGoldenFileRecord, load_java_file_metrics_golden};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
struct ComparableMainFileRecord {
    relative_path: String,
    extension: String,
    lines_of_code: usize,
    units_count: usize,
    units_mc_cabe_index_sum: usize,
    lines_of_code_in_units: usize,
}

impl From<&FileRecord> for ComparableMainFileRecord {
    fn from(file: &FileRecord) -> Self {
        Self {
            relative_path: file.relative_path.clone(),
            extension: file.extension.clone(),
            lines_of_code: file.lines_of_code,
            units_count: file.units_count,
            units_mc_cabe_index_sum: file.units_mc_cabe_index_sum,
            lines_of_code_in_units: file.lines_of_code_in_units,
        }
    }
}

impl From<&JavaGoldenFileRecord> for ComparableMainFileRecord {
    fn from(file: &JavaGoldenFileRecord) -> Self {
        Self {
            relative_path: file.relative_path.clone(),
            extension: file.extension.clone(),
            lines_of_code: file.lines_of_code,
            units_count: file.units_count,
            units_mc_cabe_index_sum: file.units_mc_cabe_index_sum,
            lines_of_code_in_units: file.lines_of_code_in_units,
        }
    }
}

fn java_units_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("java-units-sample")
}

fn java_dependencies_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("java-dependencies-sample")
}

#[test]
fn java_units_sample_matches_java_main_files_golden() -> Result<()> {
    let fixture = java_units_fixture_root();
    let config = fixture.join("input").join("_sokrates").join("config.json");
    let golden = fixture.join("goldens").join("mainFiles.json");

    let analysis = analyze_from_cli_options(None, Some(config))?;
    let expected = load_java_file_metrics_golden(&golden)?;

    let actual_summary = analysis
        .files
        .iter()
        .filter(|file| file.extension == "java")
        .map(ComparableMainFileRecord::from)
        .collect::<Vec<_>>();
    let expected_summary = expected
        .iter()
        .map(ComparableMainFileRecord::from)
        .collect::<Vec<_>>();

    assert_eq!(actual_summary, expected_summary);

    Ok(())
}

#[test]
fn java_dependencies_sample_matches_java_main_files_golden() -> Result<()> {
    let fixture = java_dependencies_fixture_root();
    let config = fixture.join("input").join("_sokrates").join("config.json");
    let golden = fixture.join("goldens").join("mainFiles.json");

    let analysis = analyze_from_cli_options(None, Some(config))?;
    let expected = load_java_file_metrics_golden(&golden)?;

    let actual_summary = analysis
        .files
        .iter()
        .filter(|file| file.extension == "java")
        .map(ComparableMainFileRecord::from)
        .collect::<Vec<_>>();
    let expected_summary = expected
        .iter()
        .map(ComparableMainFileRecord::from)
        .collect::<Vec<_>>();

    assert_eq!(actual_summary, expected_summary);

    Ok(())
}
