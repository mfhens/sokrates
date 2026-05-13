mod compat;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sokrates_ir::{
    ComponentDependency, Dependency, FileRecord, Metadata, ParseDiagnostic, RepositoryAnalysis,
    Unit,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SokratesConfig {
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub src_root: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub main: NamedSourceCodeAspectConfig,
    #[serde(default)]
    pub test: NamedSourceCodeAspectConfig,
    #[serde(default)]
    pub generated: NamedSourceCodeAspectConfig,
    #[serde(default)]
    pub build_and_deployment: NamedSourceCodeAspectConfig,
    #[serde(default)]
    pub other: NamedSourceCodeAspectConfig,
    #[serde(default)]
    pub logical_decompositions: Vec<LogicalDecompositionConfig>,
    #[serde(default)]
    pub concern_groups: Vec<ConcernGroupConfig>,
    #[serde(default)]
    pub goals_and_controls: Vec<MetricsWithGoalConfig>,
    #[serde(default)]
    pub analysis: AnalysisConfig,
    #[serde(default)]
    pub tag_rules: Vec<TagRuleConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NamedSourceCodeAspectConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_file_filters: Vec<SourceFileFilterConfig>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceFileFilterConfig {
    #[serde(default)]
    pub path_pattern: String,
    #[serde(default)]
    pub content_pattern: String,
    #[serde(default)]
    pub exception: bool,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LogicalDecompositionConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_main_scope")]
    pub scope: String,
    #[serde(default)]
    pub filters: Vec<SourceFileFilterConfig>,
    #[serde(default)]
    pub components_folder_depth: usize,
    #[serde(default)]
    pub min_components_count: usize,
    #[serde(default)]
    pub components: Vec<NamedSourceCodeAspectConfig>,
    #[serde(default)]
    pub meta_components: Vec<serde_json::Value>,
    #[serde(default)]
    pub groups: Vec<serde_json::Value>,
    #[serde(default = "default_true")]
    pub include_remaining_files: bool,
    #[serde(default)]
    pub dependencies_finder: serde_json::Value,
    #[serde(default)]
    pub rendering_options: serde_json::Value,
    #[serde(default = "default_true")]
    pub include_external_components: bool,
    #[serde(default = "default_dependency_link_threshold")]
    pub dependency_link_threshold: usize,
    #[serde(default = "default_duplication_link_threshold")]
    pub duplication_link_threshold: usize,
    #[serde(default = "default_temporal_link_threshold")]
    pub temporal_link_threshold: usize,
    #[serde(default = "default_max_search_depth_lines")]
    pub max_search_depth_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConcernGroupConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub concerns: Vec<ConcernConfig>,
    #[serde(default)]
    pub meta_concerns: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConcernConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_file_filters: Vec<SourceFileFilterConfig>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub text_operations: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetricsWithGoalConfig {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub controls: Vec<MetricRangeControlConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetricRangeControlConfig {
    #[serde(default)]
    pub metric: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub desired_range: RangeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeConfig {
    #[serde(default)]
    pub min: String,
    #[serde(default)]
    pub max: String,
    #[serde(default = "default_zero_string")]
    pub tolerance: String,
}

impl Default for RangeConfig {
    fn default() -> Self {
        Self {
            min: String::new(),
            max: String::new(),
            tolerance: default_zero_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TagRuleConfig {
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub path_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_path_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisConfig {
    #[serde(default)]
    pub skip_duplication: bool,
    #[serde(default)]
    pub skip_correlations: bool,
    #[serde(default)]
    pub skip_dependencies: bool,
    #[serde(default = "default_true")]
    pub save_source_files: bool,
    #[serde(default = "default_true")]
    pub save_code_fragments: bool,
    #[serde(default = "default_max_file_size_bytes")]
    pub max_file_size_bytes: usize,
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
    #[serde(default = "default_max_line_length")]
    pub max_line_length: usize,
    #[serde(default = "default_max_temporal_dependencies_depth_days")]
    pub max_temporal_dependencies_depth_days: usize,
    #[serde(default = "default_loc_duplication_threshold")]
    pub loc_duplication_threshold: usize,
    #[serde(default = "default_min_duplication_block_loc")]
    pub min_duplication_block_loc: usize,
    #[serde(default = "default_max_top_list_size")]
    pub max_top_list_size: usize,
    #[serde(default)]
    pub analyzer_overrides: Vec<serde_json::Value>,
    #[serde(default = "default_file_size_thresholds")]
    pub file_size_thresholds: ThresholdsConfig,
    #[serde(default = "default_file_age_thresholds")]
    pub file_age_thresholds: ThresholdsConfig,
    #[serde(default = "default_file_update_frequency_thresholds")]
    pub file_update_frequency_thresholds: ThresholdsConfig,
    #[serde(default = "default_file_contributors_count_thresholds")]
    pub file_contributors_count_thresholds: ThresholdsConfig,
    #[serde(default = "default_unit_size_thresholds")]
    pub unit_size_thresholds: ThresholdsConfig,
    #[serde(default = "default_conditional_complexity_thresholds")]
    pub conditional_complexity_thresholds: ThresholdsConfig,
    #[serde(default = "default_file_conditional_complexity_thresholds")]
    pub file_conditional_complexity_thresholds: ThresholdsConfig,
    #[serde(default = "default_commit_files_count_thresholds")]
    pub commit_files_count_thresholds: ThresholdsConfig,
    #[serde(default)]
    pub custom_html_report_header_fragment: String,
    #[serde(default)]
    pub analyze_concern_overlaps: bool,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            skip_duplication: false,
            skip_correlations: false,
            skip_dependencies: false,
            save_source_files: true,
            save_code_fragments: true,
            max_file_size_bytes: default_max_file_size_bytes(),
            max_lines: default_max_lines(),
            max_line_length: default_max_line_length(),
            max_temporal_dependencies_depth_days: default_max_temporal_dependencies_depth_days(),
            loc_duplication_threshold: default_loc_duplication_threshold(),
            min_duplication_block_loc: default_min_duplication_block_loc(),
            max_top_list_size: default_max_top_list_size(),
            analyzer_overrides: Vec::new(),
            file_size_thresholds: default_file_size_thresholds(),
            file_age_thresholds: default_file_age_thresholds(),
            file_update_frequency_thresholds: default_file_update_frequency_thresholds(),
            file_contributors_count_thresholds: default_file_contributors_count_thresholds(),
            unit_size_thresholds: default_unit_size_thresholds(),
            conditional_complexity_thresholds: default_conditional_complexity_thresholds(),
            file_conditional_complexity_thresholds: default_file_conditional_complexity_thresholds(
            ),
            commit_files_count_thresholds: default_commit_files_count_thresholds(),
            custom_html_report_header_fragment: String::new(),
            analyze_concern_overlaps: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThresholdsConfig {
    #[serde(default)]
    pub low: usize,
    #[serde(default)]
    pub medium: usize,
    #[serde(default)]
    pub high: usize,
    #[serde(default)]
    pub very_high: usize,
}

pub fn analyze_from_cli_options(
    src_root: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> Result<RepositoryAnalysis> {
    let config = config_path
        .as_ref()
        .map(|path| load_config(path))
        .transpose()?;

    let resolved_root = match (src_root, config_path.as_ref(), config.as_ref()) {
        (Some(root), _, _) => root,
        (None, Some(path), Some(config)) => resolve_src_root(path, config),
        (None, None, _) => std::env::current_dir().context("resolve current directory")?,
        (None, Some(_), None) => unreachable!("config path and config are loaded together"),
    };

    analyze_repository(&resolved_root, config.as_ref())
}

pub fn export_compat_data_from_cli_options(
    src_root: Option<PathBuf>,
    config_path: PathBuf,
    output_dir: PathBuf,
) -> Result<()> {
    let config_bytes =
        fs::read(&config_path).with_context(|| format!("read config {}", config_path.display()))?;
    let config = load_config(&config_path)?;
    let resolved_root = src_root.unwrap_or_else(|| resolve_src_root(&config_path, &config));
    let analysis = analyze_repository(&resolved_root, Some(&config))?;

    compat::export_data_bundle(
        &resolved_root,
        &config_path,
        &config_bytes,
        &config,
        &analysis,
        &output_dir,
    )
}

pub fn analyze_repository(
    src_root: &Path,
    config: Option<&SokratesConfig>,
) -> Result<RepositoryAnalysis> {
    let root = src_root
        .canonicalize()
        .with_context(|| format!("resolve source root {}", src_root.display()))?;

    if !root.is_dir() {
        anyhow::bail!("source root {} is not a directory", root.display());
    }

    let extensions = normalized_extensions(config);
    let metadata = config.map_or_else(Metadata::default, |value| value.metadata.clone());

    let mut files = WalkDir::new(&root)
        .into_iter()
        .filter_entry(should_visit_entry)
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| is_included_file(entry.path(), &extensions))
        .map(|entry| to_file_record(&root, entry.path()))
        .collect::<Result<Vec<_>>>()?;

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    files.dedup_by(|left, right| left.relative_path == right.relative_path);

    let (language_files, units, dependencies, component_dependencies, diagnostics) =
        analyze_language_specifics(&root, &files, config)?;
    merge_file_records(&mut files, language_files);
    let mut analysis = RepositoryAnalysis::with_files(metadata, files);
    analysis.units = units;
    analysis.dependencies = dependencies;
    analysis.component_dependencies = component_dependencies;
    analysis.diagnostics = diagnostics;

    Ok(analysis)
}

pub fn load_config(path: &Path) -> Result<SokratesConfig> {
    let content =
        fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;

    serde_json::from_str(&content).with_context(|| format!("parse config {}", path.display()))
}

pub fn resolve_src_root(config_path: &Path, config: &SokratesConfig) -> PathBuf {
    let parent = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let configured = if config.src_root.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(&config.src_root)
    };

    parent.join(configured)
}

fn normalized_extensions(config: Option<&SokratesConfig>) -> BTreeSet<String> {
    config
        .into_iter()
        .flat_map(|value| value.extensions.iter())
        .map(|extension| extension.to_ascii_lowercase())
        .collect()
}

fn default_main_scope() -> String {
    String::from("main")
}

fn default_true() -> bool {
    true
}

fn default_zero_string() -> String {
    String::from("0")
}

fn default_max_file_size_bytes() -> usize {
    1_000_000
}

fn default_max_lines() -> usize {
    10_000
}

fn default_max_line_length() -> usize {
    1_000
}

fn default_max_temporal_dependencies_depth_days() -> usize {
    365
}

fn default_loc_duplication_threshold() -> usize {
    10_000_000
}

fn default_min_duplication_block_loc() -> usize {
    6
}

fn default_max_top_list_size() -> usize {
    50
}

fn default_file_size_thresholds() -> ThresholdsConfig {
    ThresholdsConfig {
        low: 100,
        medium: 200,
        high: 500,
        very_high: 1_000,
    }
}

fn default_file_age_thresholds() -> ThresholdsConfig {
    ThresholdsConfig {
        low: 30,
        medium: 90,
        high: 180,
        very_high: 365,
    }
}

fn default_file_update_frequency_thresholds() -> ThresholdsConfig {
    ThresholdsConfig {
        low: 5,
        medium: 20,
        high: 50,
        very_high: 100,
    }
}

fn default_file_contributors_count_thresholds() -> ThresholdsConfig {
    ThresholdsConfig {
        low: 1,
        medium: 5,
        high: 10,
        very_high: 25,
    }
}

fn default_unit_size_thresholds() -> ThresholdsConfig {
    ThresholdsConfig {
        low: 10,
        medium: 20,
        high: 50,
        very_high: 100,
    }
}

fn default_conditional_complexity_thresholds() -> ThresholdsConfig {
    ThresholdsConfig {
        low: 5,
        medium: 10,
        high: 25,
        very_high: 50,
    }
}

fn default_file_conditional_complexity_thresholds() -> ThresholdsConfig {
    default_conditional_complexity_thresholds()
}

fn default_commit_files_count_thresholds() -> ThresholdsConfig {
    ThresholdsConfig {
        low: 5,
        medium: 20,
        high: 50,
        very_high: 100,
    }
}

fn default_dependency_link_threshold() -> usize {
    1
}

fn default_duplication_link_threshold() -> usize {
    50
}

fn default_temporal_link_threshold() -> usize {
    1
}

fn default_max_search_depth_lines() -> usize {
    200
}

fn should_visit_entry(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }

    let name = entry.file_name().to_string_lossy();

    !matches!(
        name.as_ref(),
        ".git" | "_sokrates" | "_sokrates_landscape" | "node_modules" | "target"
    )
}

fn is_included_file(path: &Path, extensions: &BTreeSet<String>) -> bool {
    if extensions.is_empty() {
        return true;
    }

    path.extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| extensions.contains(&extension))
}

fn to_file_record(root: &Path, path: &Path) -> Result<FileRecord> {
    let relative_path = path
        .strip_prefix(root)
        .with_context(|| format!("make {} relative to {}", path.display(), root.display()))?;

    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    Ok(FileRecord {
        relative_path: normalize_path(relative_path),
        extension,
        lines_of_code: 0,
        components: Vec::new(),
        concerns: Vec::new(),
        units_count: 0,
        units_mc_cabe_index_sum: 0,
        lines_of_code_in_units: 0,
    })
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>()
        .join("/")
}

fn analyze_language_specifics(
    root: &Path,
    files: &[FileRecord],
    config: Option<&SokratesConfig>,
) -> Result<(
    Vec<FileRecord>,
    Vec<Unit>,
    Vec<Dependency>,
    Vec<ComponentDependency>,
    Vec<ParseDiagnostic>,
)> {
    let mut language_files = Vec::new();
    let mut units = Vec::new();
    let mut dependencies = Vec::new();
    let mut component_dependencies = Vec::new();
    let mut diagnostics = Vec::new();
    let mut java_inputs = Vec::new();

    for file in files {
        if file.extension != "java" {
            continue;
        }

        let file_path = resolve_relative_file_path(root, &file.relative_path);
        let source = fs::read_to_string(&file_path)
            .with_context(|| format!("read source file {}", file_path.display()))?;
        let components = derive_components(&file.relative_path, config);
        let lines_of_code = source.lines().count();

        java_inputs.push(sokrates_lang_java::JavaSourceInput {
            relative_path: file.relative_path.clone(),
            file: FileRecord {
                relative_path: file.relative_path.clone(),
                extension: file.extension.clone(),
                lines_of_code,
                components,
                concerns: Vec::new(),
                units_count: 0,
                units_mc_cabe_index_sum: 0,
                lines_of_code_in_units: 0,
            },
            source,
        });
    }

    if !java_inputs.is_empty() {
        let analysis = sokrates_lang_java::analyze_repository(&java_inputs)?;
        language_files.extend(analysis.files);
        units.extend(analysis.units);
        dependencies.extend(analysis.dependencies);
        component_dependencies.extend(analysis.component_dependencies);
        diagnostics.extend(analysis.diagnostics);
    }

    units.sort_by(|left, right| right.lines_of_code.cmp(&left.lines_of_code));

    Ok((
        language_files,
        units,
        dependencies,
        component_dependencies,
        diagnostics,
    ))
}

fn merge_file_records(files: &mut [FileRecord], updates: Vec<FileRecord>) {
    let updates = updates
        .into_iter()
        .map(|file| (file.relative_path.clone(), file))
        .collect::<BTreeMap<_, _>>();

    for file in files {
        if let Some(updated) = updates.get(&file.relative_path) {
            *file = updated.clone();
        }
    }
}

fn resolve_relative_file_path(root: &Path, relative_path: &str) -> PathBuf {
    relative_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .fold(root.to_path_buf(), |path, segment| path.join(segment))
}

fn derive_components(relative_path: &str, config: Option<&SokratesConfig>) -> Vec<String> {
    let Some(config) = config else {
        return Vec::new();
    };

    let decomposition = config.logical_decompositions.first();
    let decomposition_name = decomposition
        .map(|value| value.name.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("primary");
    let depth = decomposition
        .map(|value| value.components_folder_depth)
        .filter(|depth| *depth > 0)
        .unwrap_or(1);
    let component = relative_path
        .split('/')
        .take(depth)
        .collect::<Vec<_>>()
        .join("/");

    if component.is_empty() {
        Vec::new()
    } else {
        vec![format!("{decomposition_name}::{component}")]
    }
}

#[cfg(test)]
mod tests {
    use super::analyze_from_cli_options;
    use anyhow::Result;
    use sokrates_ir::RepositoryAnalysis;
    use std::path::PathBuf;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("sample-repo")
            .join("input")
    }

    fn analyze_fixture() -> Result<RepositoryAnalysis> {
        let root = fixture_root();
        let config = root.join("_sokrates").join("config.json");

        analyze_from_cli_options(None, Some(config))
    }

    #[test]
    fn analyze_fixture_uses_config_and_normalizes_paths() -> Result<()> {
        let analysis = analyze_fixture()?;
        let paths = analysis
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "pom.xml",
                "src/main/java/demo/HelloService.java",
                "src/test/java/demo/HelloServiceTest.java",
            ]
        );
        assert_eq!(analysis.summary.total_files, 3);

        Ok(())
    }

    #[test]
    fn analyze_java_fixture_populates_units() -> Result<()> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("java-units-sample")
            .join("input");
        let config = root.join("_sokrates").join("config.json");
        let analysis = analyze_from_cli_options(None, Some(config))?;

        assert_eq!(
            analysis
                .units
                .iter()
                .map(|unit| unit.short_name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "public String classify()",
                "static",
                "public int add()",
                "public Calculator()",
            ]
        );

        Ok(())
    }

    #[test]
    fn analyze_java_fixture_populates_file_metrics() -> Result<()> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("java-units-sample")
            .join("input");
        let config = root.join("_sokrates").join("config.json");
        let analysis = analyze_from_cli_options(None, Some(config))?;

        assert_eq!(
            analysis
                .files
                .iter()
                .filter(|file| file.extension == "java")
                .map(|file| {
                    (
                        file.relative_path.as_str(),
                        file.lines_of_code,
                        file.units_count,
                        file.units_mc_cabe_index_sum,
                        file.lines_of_code_in_units,
                    )
                })
                .collect::<Vec<_>>(),
            vec![("src/main/java/app/Calculator.java", 19, 4, 7, 16)]
        );

        Ok(())
    }

    #[test]
    fn analyze_dependency_fixture_populates_package_and_component_dependencies() -> Result<()> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("java-dependencies-sample")
            .join("input");
        let config = root.join("_sokrates").join("config.json");
        let analysis = analyze_from_cli_options(None, Some(config))?;

        assert_eq!(
            analysis
                .dependencies
                .iter()
                .map(|dependency| (dependency.from.as_str(), dependency.to.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("alpha.api", "alpha.internal"),
                ("alpha.api", "beta.api"),
                ("alpha.api", "beta.internal"),
            ]
        );
        assert_eq!(
            analysis
                .component_dependencies
                .iter()
                .map(|dependency| {
                    (
                        dependency.decomposition.as_str(),
                        dependency.from_component.as_str(),
                        dependency.to_component.as_str(),
                        dependency.count,
                        dependency.loc_from,
                    )
                })
                .collect::<Vec<_>>(),
            vec![("primary", "alpha", "beta", 1, 11)]
        );
        assert_eq!(
            analysis
                .files
                .iter()
                .filter(|file| file.extension == "java")
                .map(|file| {
                    (
                        file.relative_path.as_str(),
                        file.lines_of_code,
                        file.units_count,
                        file.units_mc_cabe_index_sum,
                        file.lines_of_code_in_units,
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("alpha/api/AlphaService.java", 11, 1, 1, 4),
                ("alpha/internal/AlphaHelper.java", 6, 1, 1, 3),
                ("beta/api/BetaFacade.java", 6, 1, 1, 3),
                ("beta/api/BetaService.java", 9, 2, 2, 6),
                ("beta/internal/BetaHelper.java", 6, 1, 1, 3),
            ]
        );

        Ok(())
    }
}
