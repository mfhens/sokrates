use anyhow::{Context, Result};
use serde_json::{Value, json};
use sokrates_cli::export_compat_data_from_cli_options;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join(name)
}

fn unique_output_dir(test_name: &str) -> Result<PathBuf> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("compute unix timestamp for temp output")?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "sokrates-compat-{test_name}-{}-{suffix}",
        std::process::id()
    )))
}

fn read_json(path: &Path) -> Result<Value> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))
}

fn read_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("read {}", path.display()))
}

fn assert_json_matches(actual: &Path, expected: &Path) -> Result<()> {
    assert_eq!(read_json(actual)?, read_json(expected)?);
    Ok(())
}

fn collect_relative_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_relative_files_recursive(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_relative_files_recursive(
    root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(current).with_context(|| format!("list {}", current.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", current.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_relative_files_recursive(root, &path, files)?;
        } else if path.is_file() {
            files.push(
                path.strip_prefix(root)
                    .with_context(|| {
                        format!("strip prefix {} from {}", root.display(), path.display())
                    })?
                    .to_path_buf(),
            );
        }
    }

    Ok(())
}

fn normalize_analysis_results(mut value: Value) -> Value {
    if let Some(start_time) = value.get_mut("analysisStartTimeMs") {
        *start_time = json!(0);
    }

    if let Some(metrics) = value
        .get_mut("metricsList")
        .and_then(|metrics_list| metrics_list.get_mut("metrics"))
        .and_then(Value::as_array_mut)
    {
        for metric in metrics {
            if metric.get("id") == Some(&json!("TOTAL_ANALYSIS_TIME_IN_MILLIS")) {
                if let Some(metric_value) = metric.get_mut("value") {
                    *metric_value = json!(0);
                }
            }
        }
    }

    value
}

fn assert_analysis_results_match(actual: &Path, expected: &Path) -> Result<()> {
    assert_eq!(
        normalize_analysis_results(read_json(actual)?),
        normalize_analysis_results(read_json(expected)?)
    );
    Ok(())
}

fn normalize_duplicates_export(mut value: Value) -> Value {
    if let Some(timestamp) = value.get_mut("timestamp") {
        *timestamp = json!("1970-01-01 00:00:00");
    }

    value
}

fn assert_duplicates_match(actual: &Path, expected: &Path) -> Result<()> {
    assert_eq!(
        normalize_duplicates_export(read_json(actual)?),
        normalize_duplicates_export(read_json(expected)?)
    );
    Ok(())
}

fn normalize_text(relative_path: &Path, content: String) -> String {
    let mut normalized = content.replace("\r\n", "\n");
    let file_name = relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if file_name == "metrics.txt" {
        normalized = normalized
            .lines()
            .map(|line| {
                if line.starts_with("TOTAL_ANALYSIS_TIME_IN_MILLIS: ") {
                    String::from("TOTAL_ANALYSIS_TIME_IN_MILLIS: 0")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }

    if file_name == "metrics_trend.txt" {
        normalized = normalized
            .lines()
            .map(|line| {
                if line.starts_with("TOTAL_ANALYSIS_TIME_IN_MILLIS\t\t") {
                    String::from("TOTAL_ANALYSIS_TIME_IN_MILLIS\t\t0")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }

    if file_name == "textualSummary.txt" {
        normalized = normalized
            .lines()
            .map(|line| {
                if line.starts_with("Total analysis time: ") {
                    String::from("Total analysis time: .00s")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
    }

    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }

    normalized
}

fn assert_text_matches(actual: &Path, expected: &Path, relative_path: &Path) -> Result<()> {
    assert_eq!(
        normalize_text(relative_path, read_text(actual)?),
        normalize_text(relative_path, read_text(expected)?)
    );
    Ok(())
}

fn assert_text_tree_matches(actual_root: &Path, expected_root: &Path) -> Result<()> {
    let expected_files = collect_relative_files(expected_root)?;
    let actual_files = collect_relative_files(actual_root)?;
    assert_eq!(actual_files, expected_files);

    for relative_path in expected_files {
        assert_text_matches(
            &actual_root.join(&relative_path),
            &expected_root.join(&relative_path),
            &relative_path,
        )?;
    }

    Ok(())
}

fn assert_scope_path_jsons_match(output_dir: &Path, goldens: &Path) -> Result<()> {
    for file_name in [
        "mainFilesPaths.json",
        "testFilesPaths.json",
        "generatedFilesPaths.json",
        "buildAndDeploymentFilesPaths.json",
        "otherFilesPaths.json",
    ] {
        assert_json_matches(&output_dir.join(file_name), &goldens.join(file_name))?;
    }

    Ok(())
}

fn assert_execution_times_present(output_dir: &Path) {
    assert!(output_dir.join("executionTimes.json").exists());
    assert!(output_dir.join("executionTimes.txt").exists());
}

fn assert_all_files_zip_matches_text(output_dir: &Path) -> Result<()> {
    let zip_path = output_dir.join("zips").join("all_files.zip");
    assert!(zip_path.exists());

    let file = fs::File::open(&zip_path).with_context(|| format!("open {}", zip_path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("read {}", zip_path.display()))?;
    let expected_entries = [
        "aspect_main.txt",
        "aspect_test.txt",
        "aspect_generated.txt",
        "aspect_build_and_deployment.txt",
        "aspect_other.txt",
    ];

    let mut actual_entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("read zip entry #{index} from {}", zip_path.display()))?;
        actual_entries.push(entry.name().to_string());
    }
    assert_eq!(
        actual_entries,
        expected_entries
            .iter()
            .map(|entry| entry.to_string())
            .collect::<Vec<_>>()
    );

    for entry_name in expected_entries {
        let mut entry = archive
            .by_name(entry_name)
            .with_context(|| format!("read {entry_name} from {}", zip_path.display()))?;
        let mut zipped_content = String::new();
        entry
            .read_to_string(&mut zipped_content)
            .with_context(|| format!("read {entry_name} content from {}", zip_path.display()))?;

        assert_eq!(
            normalize_text(Path::new(entry_name), zipped_content),
            normalize_text(
                Path::new(entry_name),
                read_text(&output_dir.join("text").join(entry_name))?
            )
        );
    }

    Ok(())
}

#[test]
fn sample_repo_export_matches_java_scope_goldens() -> Result<()> {
    let fixture = fixture_root("sample-repo");
    let config = fixture.join("input").join("_sokrates").join("config.json");
    let goldens = fixture.join("goldens");
    let output_dir = unique_output_dir("sample-repo")?;

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("cleanup {}", output_dir.display()))?;
    }

    export_compat_data_from_cli_options(None, config.clone(), output_dir.clone())?;

    assert_eq!(
        fs::read(output_dir.join("config.json"))?,
        fs::read(&config)?,
        "config.json should be a byte copy of the source config"
    );
    assert_json_matches(
        &output_dir.join("mainFiles.json"),
        &goldens.join("mainFiles.json"),
    )?;
    assert_json_matches(
        &output_dir.join("testFiles.json"),
        &goldens.join("testFiles.json"),
    )?;
    assert_json_matches(
        &output_dir.join("generatedFiles.json"),
        &goldens.join("generatedFiles.json"),
    )?;
    assert_json_matches(
        &output_dir.join("buildAndDeploymentFiles.json"),
        &goldens.join("buildAndDeploymentFiles.json"),
    )?;
    assert_json_matches(
        &output_dir.join("otherFiles.json"),
        &goldens.join("otherFiles.json"),
    )?;
    assert_eq!(read_json(&output_dir.join("files.json"))?, json!([]));
    assert_json_matches(
        &output_dir.join("concerns.json"),
        &goldens.join("concerns.json"),
    )?;
    assert_json_matches(
        &output_dir.join("contributors.json"),
        &goldens.join("contributors.json"),
    )?;
    assert_duplicates_match(
        &output_dir.join("duplicates.json"),
        &goldens.join("duplicates.json"),
    )?;
    assert_analysis_results_match(
        &output_dir.join("analysisResults.json"),
        &goldens.join("analysisResults.json"),
    )?;
    assert_scope_path_jsons_match(&output_dir, &goldens)?;
    assert_text_tree_matches(&output_dir.join("text"), &goldens.join("text"))?;
    assert_execution_times_present(&output_dir);
    assert_all_files_zip_matches_text(&output_dir)?;

    fs::remove_dir_all(&output_dir).with_context(|| format!("cleanup {}", output_dir.display()))?;
    Ok(())
}

#[test]
fn java_units_export_matches_java_goldens() -> Result<()> {
    let fixture = fixture_root("java-units-sample");
    let config = fixture.join("input").join("_sokrates").join("config.json");
    let goldens = fixture.join("goldens");
    let output_dir = unique_output_dir("java-units")?;

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("cleanup {}", output_dir.display()))?;
    }

    export_compat_data_from_cli_options(None, config, output_dir.clone())?;

    assert_json_matches(
        &output_dir.join("mainFiles.json"),
        &goldens.join("mainFiles.json"),
    )?;
    assert_json_matches(
        &output_dir.join("buildAndDeploymentFiles.json"),
        &goldens.join("buildAndDeploymentFiles.json"),
    )?;
    assert_json_matches(&output_dir.join("units.json"), &goldens.join("units.json"))?;
    assert_eq!(
        read_json(&output_dir.join("files.json"))?,
        json!([
            {
                "relativePath": "src/main/java/app/Calculator.java",
                "extension": "java",
                "linesOfCode": 19,
                "components": ["primary::src"],
                "concerns": ["::Unclassified"]
            }
        ])
    );
    assert_json_matches(
        &output_dir.join("concerns.json"),
        &goldens.join("concerns.json"),
    )?;
    assert_json_matches(
        &output_dir.join("contributors.json"),
        &goldens.join("contributors.json"),
    )?;
    assert_duplicates_match(
        &output_dir.join("duplicates.json"),
        &goldens.join("duplicates.json"),
    )?;
    assert_analysis_results_match(
        &output_dir.join("analysisResults.json"),
        &goldens.join("analysisResults.json"),
    )?;
    assert_scope_path_jsons_match(&output_dir, &goldens)?;
    assert_text_tree_matches(&output_dir.join("text"), &goldens.join("text"))?;
    assert_execution_times_present(&output_dir);
    assert_all_files_zip_matches_text(&output_dir)?;

    fs::remove_dir_all(&output_dir).with_context(|| format!("cleanup {}", output_dir.display()))?;
    Ok(())
}

#[test]
fn java_dependencies_export_matches_java_goldens() -> Result<()> {
    let fixture = fixture_root("java-dependencies-sample");
    let config = fixture.join("input").join("_sokrates").join("config.json");
    let goldens = fixture.join("goldens");
    let output_dir = unique_output_dir("java-dependencies")?;

    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)
            .with_context(|| format!("cleanup {}", output_dir.display()))?;
    }

    export_compat_data_from_cli_options(None, config, output_dir.clone())?;

    assert_json_matches(
        &output_dir.join("mainFiles.json"),
        &goldens.join("mainFiles.json"),
    )?;
    assert_json_matches(
        &output_dir.join("dependencies.json"),
        &goldens.join("dependencies.json"),
    )?;
    assert_json_matches(
        &output_dir.join("logical_decompositions.json"),
        &goldens.join("logical_decompositions.json"),
    )?;
    assert_eq!(
        read_json(&output_dir.join("files.json"))?,
        json!([
            {
                "relativePath": "alpha/api/AlphaService.java",
                "extension": "java",
                "linesOfCode": 11,
                "components": ["primary::alpha"],
                "concerns": ["::Unclassified"]
            },
            {
                "relativePath": "alpha/internal/AlphaHelper.java",
                "extension": "java",
                "linesOfCode": 6,
                "components": ["primary::alpha"],
                "concerns": ["::Unclassified"]
            },
            {
                "relativePath": "beta/api/BetaFacade.java",
                "extension": "java",
                "linesOfCode": 6,
                "components": ["primary::beta"],
                "concerns": ["::Unclassified"]
            },
            {
                "relativePath": "beta/api/BetaService.java",
                "extension": "java",
                "linesOfCode": 9,
                "components": ["primary::beta"],
                "concerns": ["::Unclassified"]
            },
            {
                "relativePath": "beta/internal/BetaHelper.java",
                "extension": "java",
                "linesOfCode": 6,
                "components": ["primary::beta"],
                "concerns": ["::Unclassified"]
            }
        ])
    );
    assert_json_matches(
        &output_dir.join("concerns.json"),
        &goldens.join("concerns.json"),
    )?;
    assert_json_matches(
        &output_dir.join("contributors.json"),
        &goldens.join("contributors.json"),
    )?;
    assert_duplicates_match(
        &output_dir.join("duplicates.json"),
        &goldens.join("duplicates.json"),
    )?;
    assert_analysis_results_match(
        &output_dir.join("analysisResults.json"),
        &goldens.join("analysisResults.json"),
    )?;
    assert_scope_path_jsons_match(&output_dir, &goldens)?;
    assert_text_tree_matches(&output_dir.join("text"), &goldens.join("text"))?;
    assert_execution_times_present(&output_dir);
    assert_all_files_zip_matches_text(&output_dir)?;

    fs::remove_dir_all(&output_dir).with_context(|| format!("cleanup {}", output_dir.display()))?;
    Ok(())
}
