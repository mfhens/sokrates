use anyhow::Result;
use sokrates_cli::analyze_from_cli_options;
use sokrates_ir::Unit;
use sokrates_parity::load_java_units_golden;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
struct ComparableUnit {
    short_name: String,
    long_name: String,
    relative_file_name: String,
    file_lines_count: usize,
    components: Vec<String>,
    start_line: usize,
    end_line: usize,
    lines_of_code: usize,
    mc_cabe_index: usize,
    number_of_parameters: usize,
    number_of_literals: usize,
    number_of_statements: usize,
    number_of_expressions: usize,
}

impl From<&Unit> for ComparableUnit {
    fn from(unit: &Unit) -> Self {
        Self {
            short_name: unit.short_name.clone(),
            long_name: unit.long_name.clone(),
            relative_file_name: unit.relative_file_name.clone(),
            file_lines_count: unit.file_lines_count,
            components: unit.components.clone(),
            start_line: unit.start_line,
            end_line: unit.end_line,
            lines_of_code: unit.lines_of_code,
            mc_cabe_index: unit.mc_cabe_index,
            number_of_parameters: unit.number_of_parameters,
            number_of_literals: unit.number_of_literals,
            number_of_statements: unit.number_of_statements,
            number_of_expressions: unit.number_of_expressions,
        }
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("java-units-sample")
}

#[test]
fn java_units_sample_matches_java_units_golden() -> Result<()> {
    let fixture = fixture_root();
    let config = fixture.join("input").join("_sokrates").join("config.json");
    let golden = fixture.join("goldens").join("units.json");

    let analysis = analyze_from_cli_options(None, Some(config))?;
    let expected = load_java_units_golden(&golden)?;

    let actual_summary = analysis
        .units
        .iter()
        .map(ComparableUnit::from)
        .collect::<Vec<_>>();
    let expected_summary = expected
        .iter()
        .map(ComparableUnit::from)
        .collect::<Vec<_>>();

    assert_eq!(actual_summary, expected_summary);

    Ok(())
}
