use crate::{
    ConcernConfig, LogicalDecompositionConfig, MetricsWithGoalConfig, NamedSourceCodeAspectConfig,
    RangeConfig, RepositoryAnalysis, SokratesConfig, SourceFileFilterConfig, TagRuleConfig,
    ThresholdsConfig, resolve_relative_file_path,
};
use anyhow::{Context, Result};
use chrono::Local;
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};
use sokrates_ir::{ComponentDependency, Dependency, FileRecord, Unit};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use zip::ZipWriter;
use zip::write::FileOptions;

const UNCLASSIFIED_FILES: &str = "Unclassified";
const MULTIPLE_CLASSIFICATIONS: &str = "Multiple Classifications";
const MAX_COMPONENT_SEARCH_DEPTH: usize = 20;

pub fn export_data_bundle(
    root: &Path,
    _config_path: &Path,
    config_bytes: &[u8],
    config: &SokratesConfig,
    analysis: &RepositoryAnalysis,
    output_dir: &Path,
) -> Result<()> {
    let export_started_at = current_unix_millis()?;
    let export_timer = Instant::now();
    fs::create_dir_all(output_dir)
        .with_context(|| format!("create export directory {}", output_dir.display()))?;
    fs::create_dir_all(output_dir.join("text"))
        .with_context(|| format!("create text directory {}", output_dir.display()))?;
    fs::create_dir_all(output_dir.join("zips"))
        .with_context(|| format!("create zip directory {}", output_dir.display()))?;
    fs::create_dir_all(output_dir.join("extra_analysis"))
        .with_context(|| format!("create extra analysis directory {}", output_dir.display()))?;
    fs::write(output_dir.join("config.json"), config_bytes)
        .with_context(|| format!("write config copy to {}", output_dir.display()))?;

    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolve source root {}", root.display()))?;
    let contexts = build_file_contexts(&canonical_root, &analysis.files)?;
    let scopes = classify_scopes(&contexts, analysis, config)?;
    let concern_groups = analyze_concern_groups(&contexts, &scopes.main, config)?;
    let concerns = concern_labels_from_groups(&scopes.main, &concern_groups);
    let main_units = filter_units(&analysis.units, &scopes.main);
    let main_dependencies = filter_dependencies(&analysis.dependencies, &scopes.main);

    write_json(
        &output_dir.join("mainFiles.json"),
        &collect_source_files(&analysis.files, &contexts, &scopes.main, true),
    )?;
    write_json(
        &output_dir.join("testFiles.json"),
        &collect_source_files(&analysis.files, &contexts, &scopes.test, false),
    )?;
    write_json(
        &output_dir.join("generatedFiles.json"),
        &collect_source_files(&analysis.files, &contexts, &scopes.generated, false),
    )?;
    write_json(
        &output_dir.join("buildAndDeploymentFiles.json"),
        &collect_source_files(
            &analysis.files,
            &contexts,
            &scopes.build_and_deployment,
            false,
        ),
    )?;
    write_json(
        &output_dir.join("otherFiles.json"),
        &collect_source_files(&analysis.files, &contexts, &scopes.other, false),
    )?;
    write_json(
        &output_dir.join("files.json"),
        &collect_file_export_infos(&analysis.files, &contexts, &scopes.main, &concerns),
    )?;
    write_json(&output_dir.join("units.json"), &export_units(&main_units))?;
    write_json(
        &output_dir.join("dependencies.json"),
        &export_dependencies(&main_dependencies, &concerns),
    )?;
    let logical_decompositions = export_logical_decompositions(
        &canonical_root,
        &analysis.files,
        &contexts,
        &scopes,
        config,
        &analysis.component_dependencies,
    );
    write_json(
        &output_dir.join("logical_decompositions.json"),
        &logical_decompositions,
    )?;
    let concerns_analysis = export_concerns_analysis(&analysis.files, &contexts, &concern_groups);
    write_json(&output_dir.join("concerns.json"), &concerns_analysis)?;
    write_json(
        &output_dir.join("contributors.json"),
        &build_contributors_export(),
    )?;
    write_json(
        &output_dir.join("duplicates.json"),
        &build_duplicates_export(),
    )?;
    let analysis_time_ms = export_timer.elapsed().as_millis() as u64;
    let bundle = build_analysis_results_bundle(
        &analysis.files,
        &contexts,
        &scopes,
        config,
        &main_units,
        &main_dependencies,
        &concerns_analysis,
        &logical_decompositions,
        analysis_time_ms,
    );
    let analysis_results = build_analysis_results_export(
        &analysis.files,
        config,
        &config.metadata,
        analysis.files.len(),
        &concerns_analysis,
        &logical_decompositions,
        &bundle,
        export_started_at,
    );
    write_json(&output_dir.join("analysisResults.json"), &analysis_results)?;
    write_java_text_and_support_artifacts(
        output_dir,
        config,
        &analysis.files,
        &contexts,
        &scopes,
        &concern_groups,
        &logical_decompositions,
        &main_units,
        &main_dependencies,
        &bundle,
        analysis_time_ms,
        export_started_at,
        export_timer.elapsed().as_millis() as u64,
    )?;

    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)
        .with_context(|| format!("serialize {}", path.display()))?;
    fs::write(path, json).with_context(|| format!("write {}", path.display()))
}

#[derive(Debug, Clone)]
struct ExportFileContext {
    file: FileRecord,
    absolute_path: PathBuf,
    lines: Vec<String>,
}

fn build_file_contexts(
    root: &Path,
    files: &[FileRecord],
) -> Result<BTreeMap<String, ExportFileContext>> {
    let mut contexts = BTreeMap::new();

    for file in files {
        let absolute_path = resolve_relative_file_path(root, &file.relative_path);
        let content = fs::read_to_string(&absolute_path)
            .with_context(|| format!("read source file {}", absolute_path.display()))?;
        let lines = split_lines(&content);
        let mut export_file = file.clone();

        if export_file.lines_of_code == 0 {
            export_file.lines_of_code = count_non_empty_lines(&lines);
        }

        contexts.insert(
            export_file.relative_path.clone(),
            ExportFileContext {
                file: export_file,
                absolute_path,
                lines,
            },
        );
    }

    Ok(contexts)
}

fn split_lines(content: &str) -> Vec<String> {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::to_owned)
        .collect()
}

fn count_non_empty_lines(lines: &[String]) -> usize {
    lines.iter().filter(|line| !line.trim().is_empty()).count()
}

#[derive(Debug, Default)]
struct ScopeFiles {
    main: BTreeSet<String>,
    test: BTreeSet<String>,
    generated: BTreeSet<String>,
    build_and_deployment: BTreeSet<String>,
    other: BTreeSet<String>,
}

fn classify_scopes(
    contexts: &BTreeMap<String, ExportFileContext>,
    analysis: &RepositoryAnalysis,
    config: &SokratesConfig,
) -> Result<ScopeFiles> {
    let mut scopes = ScopeFiles {
        main: match_aspect(contexts, &config.main)?,
        test: match_aspect(contexts, &config.test)?,
        generated: match_aspect(contexts, &config.generated)?,
        build_and_deployment: match_aspect(contexts, &config.build_and_deployment)?,
        other: match_aspect(contexts, &config.other)?,
    };

    remove_paths(&mut scopes.main, &scopes.test);
    remove_paths(&mut scopes.main, &scopes.generated);
    remove_paths(&mut scopes.main, &scopes.build_and_deployment);
    remove_paths(&mut scopes.main, &scopes.other);
    remove_paths(&mut scopes.build_and_deployment, &scopes.other);
    remove_paths(&mut scopes.build_and_deployment, &scopes.generated);
    remove_paths(&mut scopes.build_and_deployment, &scopes.test);
    remove_paths(&mut scopes.test, &scopes.other);
    remove_paths(&mut scopes.test, &scopes.generated);

    let known_paths = analysis
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<BTreeSet<_>>();
    for scope_paths in [
        &scopes.main,
        &scopes.test,
        &scopes.generated,
        &scopes.build_and_deployment,
        &scopes.other,
    ] {
        for path in scope_paths {
            if !known_paths.contains(path.as_str()) {
                anyhow::bail!("scope classification produced unknown file path {path}");
            }
        }
    }

    Ok(scopes)
}

fn remove_paths(target: &mut BTreeSet<String>, removed: &BTreeSet<String>) {
    for path in removed {
        target.remove(path);
    }
}

fn match_aspect(
    contexts: &BTreeMap<String, ExportFileContext>,
    aspect: &NamedSourceCodeAspectConfig,
) -> Result<BTreeSet<String>> {
    let mut matches = BTreeSet::new();

    for (relative_path, context) in contexts {
        let mut included = aspect.files.iter().any(|file| file == relative_path);
        let mut excluded = false;

        for filter in &aspect.source_file_filters {
            if filter_matches(filter, context)? {
                if filter.exception {
                    excluded = true;
                } else {
                    included = true;
                }
            }
        }

        if included && !excluded {
            matches.insert(relative_path.clone());
        }
    }

    Ok(matches)
}

fn filter_matches(filter: &SourceFileFilterConfig, context: &ExportFileContext) -> Result<bool> {
    Ok(path_matches(&filter.path_pattern, &context.absolute_path)?
        && content_matches(&filter.content_pattern, &context.lines)?)
}

fn path_matches(pattern: &str, path: &Path) -> Result<bool> {
    if pattern.trim().is_empty() {
        return Ok(true);
    }

    let path_text = path.to_string_lossy().to_string();
    let normalized_path = path_text.replace('\\', "/");
    let reversed_path = path_text.replace('/', "\\");
    let normalized_pattern = pattern.replace('\\', "/");

    Ok(matches_entire(pattern, &path_text)?
        || matches_entire(pattern, &normalized_path)?
        || matches_entire(pattern, &reversed_path)?
        || matches_entire(&normalized_pattern, &normalized_path)?
        || matches_entire(&normalized_pattern, &reversed_path)?)
}

fn content_matches(pattern: &str, lines: &[String]) -> Result<bool> {
    if pattern.trim().is_empty() {
        return Ok(true);
    }

    for line in lines {
        if matches_entire(pattern, line)? {
            return Ok(true);
        }
    }

    Ok(false)
}

fn matches_entire(pattern: &str, value: &str) -> Result<bool> {
    let regex = match Regex::new(&format!("^(?:{pattern})$")) {
        Ok(regex) => regex,
        Err(_) => return Ok(false),
    };

    Ok(regex.is_match(value))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SourceFileExport {
    relative_path: String,
    extension: String,
    lines_of_code: usize,
    units_count: usize,
    units_mc_cabe_index_sum: usize,
    lines_of_code_in_units: usize,
}

fn collect_source_files(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    selected_paths: &BTreeSet<String>,
    include_unit_metrics: bool,
) -> Vec<SourceFileExport> {
    files
        .iter()
        .filter(|file| selected_paths.contains(&file.relative_path))
        .filter_map(|file| contexts.get(&file.relative_path))
        .map(|context| source_file_export_from_context(context, include_unit_metrics))
        .collect()
}

fn source_file_export_from_context(
    context: &ExportFileContext,
    include_unit_metrics: bool,
) -> SourceFileExport {
    SourceFileExport {
        relative_path: context.file.relative_path.clone(),
        extension: context.file.extension.clone(),
        lines_of_code: context.file.lines_of_code,
        units_count: if include_unit_metrics {
            context.file.units_count
        } else {
            0
        },
        units_mc_cabe_index_sum: if include_unit_metrics {
            context.file.units_mc_cabe_index_sum
        } else {
            0
        },
        lines_of_code_in_units: if include_unit_metrics {
            context.file.lines_of_code_in_units
        } else {
            0
        },
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct FileExportInfoExport {
    relative_path: String,
    extension: String,
    lines_of_code: usize,
    components: Vec<String>,
    concerns: Vec<String>,
}

fn collect_file_export_infos(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    selected_paths: &BTreeSet<String>,
    concerns: &BTreeMap<String, Vec<String>>,
) -> Vec<FileExportInfoExport> {
    files
        .iter()
        .filter(|file| selected_paths.contains(&file.relative_path))
        .filter_map(|file| contexts.get(&file.relative_path))
        .map(|context| file_export_info_from_record(&context.file, concerns))
        .collect()
}

fn file_export_info_from_record(
    file: &FileRecord,
    concerns: &BTreeMap<String, Vec<String>>,
) -> FileExportInfoExport {
    FileExportInfoExport {
        relative_path: file.relative_path.clone(),
        extension: file.extension.clone(),
        lines_of_code: file.lines_of_code,
        components: file.components.clone(),
        concerns: concerns
            .get(&file.relative_path)
            .cloned()
            .unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct UnitExport {
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

fn export_units(units: &[Unit]) -> Vec<UnitExport> {
    units
        .iter()
        .map(|unit| UnitExport {
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
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DependencyExport {
    from_files: Vec<SourceFileDependencyExport>,
    from: DependencyAnchorExport,
    to: DependencyAnchorExport,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DependencyAnchorExport {
    anchor: String,
    code_fragment: String,
    dependency_patterns: Vec<String>,
    files: Vec<FileExportInfoExport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SourceFileDependencyExport {
    file: FileExportInfoExport,
    code_fragment: String,
}

fn export_dependencies(
    dependencies: &[Dependency],
    concerns: &BTreeMap<String, Vec<String>>,
) -> Vec<DependencyExport> {
    dependencies
        .iter()
        .map(|dependency| DependencyExport {
            from_files: dependency
                .from_files
                .iter()
                .map(|from_file| SourceFileDependencyExport {
                    file: file_export_info_from_record(&from_file.file, concerns),
                    code_fragment: from_file.code_fragment.clone(),
                })
                .collect(),
            from: DependencyAnchorExport {
                anchor: dependency.from_anchor.anchor.clone(),
                code_fragment: dependency.from_anchor.code_fragment.clone(),
                dependency_patterns: dependency.from_anchor.dependency_patterns.clone(),
                files: dependency
                    .from_anchor
                    .files
                    .iter()
                    .map(|file| file_export_info_from_record(file, concerns))
                    .collect(),
            },
            to: DependencyAnchorExport {
                anchor: dependency.to_anchor.anchor.clone(),
                code_fragment: dependency.to_anchor.code_fragment.clone(),
                dependency_patterns: dependency.to_anchor.dependency_patterns.clone(),
                files: dependency
                    .to_anchor
                    .files
                    .iter()
                    .map(|file| file_export_info_from_record(file, concerns))
                    .collect(),
            },
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogicalDecompositionExport {
    key: String,
    components: Vec<AspectAnalysisExport>,
    component_dependencies: Vec<ComponentDependencyExport>,
    component_dependencies_errors: Vec<serde_json::Value>,
    logical_decomposition: LogicalDecompositionConfig,
    lines_of_code_per_component: Vec<NumericMetricExport>,
    file_count_per_component: Vec<NumericMetricExport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AspectAnalysisExport {
    name: String,
    files_count: usize,
    lines_of_code: usize,
    number_of_regex_line_matches: usize,
    file_count_per_extension: Vec<NumericMetricExport>,
    lines_of_code_per_extension: Vec<NumericMetricExport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NumericMetricExport {
    name: String,
    value: usize,
    description: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComponentDependencyExport {
    from_component: String,
    loc_from: usize,
    value_from: f64,
    value_to: f64,
    evidence: Vec<ComponentDependencyEvidenceExport>,
    to_component: String,
    count: usize,
    text: Option<String>,
    color: String,
    dependency_string: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ComponentDependencyEvidenceExport {
    path_from: String,
    evidence: String,
}

fn export_logical_decompositions(
    root: &Path,
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    scopes: &ScopeFiles,
    config: &SokratesConfig,
    component_dependencies: &[ComponentDependency],
) -> Vec<LogicalDecompositionExport> {
    config
        .logical_decompositions
        .iter()
        .map(|decomposition| {
            let scope_paths = scope_paths(scopes, &decomposition.scope);
            let component_stats =
                build_component_stats(files, contexts, scope_paths, &decomposition.name);
            let mut logical_decomposition = decomposition.clone();

            if logical_decomposition.components_folder_depth > 0 {
                logical_decomposition.components = generate_folder_depth_components(
                    root,
                    files,
                    scope_paths,
                    logical_decomposition.components_folder_depth,
                    logical_decomposition.min_components_count,
                );
            }

            LogicalDecompositionExport {
                key: decomposition.name.clone(),
                components: component_stats
                    .values()
                    .map(|stats| stats.aspect.clone())
                    .collect(),
                component_dependencies: component_dependencies
                    .iter()
                    .filter(|dependency| dependency.decomposition == decomposition.name)
                    .map(export_component_dependency)
                    .collect(),
                component_dependencies_errors: Vec::new(),
                logical_decomposition,
                lines_of_code_per_component: component_stats
                    .values()
                    .map(|stats| NumericMetricExport {
                        name: stats.aspect.name.clone(),
                        value: stats.aspect.lines_of_code,
                        description: Vec::new(),
                    })
                    .collect(),
                file_count_per_component: component_stats
                    .values()
                    .map(|stats| NumericMetricExport {
                        name: stats.aspect.name.clone(),
                        value: stats.aspect.files_count,
                        description: Vec::new(),
                    })
                    .collect(),
            }
        })
        .collect()
}

fn scope_paths<'a>(scopes: &'a ScopeFiles, scope: &str) -> &'a BTreeSet<String> {
    match scope {
        "test" => &scopes.test,
        "generated" => &scopes.generated,
        "buildAndDeployment" => &scopes.build_and_deployment,
        "other" => &scopes.other,
        _ => &scopes.main,
    }
}

#[derive(Debug, Clone)]
struct ComponentStats {
    aspect: AspectAnalysisExport,
}

fn build_component_stats(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    scope_paths: &BTreeSet<String>,
    decomposition: &str,
) -> BTreeMap<String, ComponentStats> {
    let mut grouped = BTreeMap::<String, Vec<FileRecord>>::new();

    for file in files
        .iter()
        .filter(|file| scope_paths.contains(&file.relative_path))
    {
        for component in &file.components {
            let Some((component_decomposition, component_name)) = component.split_once("::") else {
                continue;
            };
            if component_decomposition != decomposition {
                continue;
            }

            let context = contexts
                .get(&file.relative_path)
                .map(|context| context.file.clone())
                .unwrap_or_else(|| file.clone());
            grouped
                .entry(component_name.to_string())
                .or_default()
                .push(context);
        }
    }

    grouped
        .into_iter()
        .map(|(component_name, component_files)| {
            let lines_of_code = component_files.iter().map(|file| file.lines_of_code).sum();
            let aspect = AspectAnalysisExport {
                name: component_name.clone(),
                files_count: component_files.len(),
                lines_of_code,
                number_of_regex_line_matches: 0,
                file_count_per_extension: extension_metrics(&component_files, |group| group.len()),
                lines_of_code_per_extension: extension_metrics(&component_files, |group| {
                    group.iter().map(|file| file.lines_of_code).sum()
                }),
            };

            (component_name, ComponentStats { aspect })
        })
        .collect()
}

fn extension_metrics(
    files: &[FileRecord],
    value_fn: impl Fn(&[FileRecord]) -> usize,
) -> Vec<NumericMetricExport> {
    let mut groups = BTreeMap::<String, Vec<FileRecord>>::new();
    for file in files {
        groups
            .entry(file.extension.to_ascii_lowercase())
            .or_default()
            .push(file.clone());
    }

    let mut metrics = groups
        .into_iter()
        .map(|(extension, files)| NumericMetricExport {
            name: format!("  *.{extension}"),
            value: value_fn(&files),
            description: Vec::new(),
        })
        .collect::<Vec<_>>();
    metrics.sort_by(|left, right| {
        right
            .value
            .cmp(&left.value)
            .then_with(|| left.name.cmp(&right.name))
    });
    metrics
}

fn export_component_dependency(dependency: &ComponentDependency) -> ComponentDependencyExport {
    ComponentDependencyExport {
        from_component: dependency.from_component.clone(),
        loc_from: dependency.loc_from,
        value_from: 0.0,
        value_to: 0.0,
        evidence: dependency
            .evidence
            .iter()
            .map(|evidence| ComponentDependencyEvidenceExport {
                path_from: evidence.path_from.clone(),
                evidence: evidence.evidence.clone(),
            })
            .collect(),
        to_component: dependency.to_component.clone(),
        count: dependency.count,
        text: None,
        color: String::new(),
        dependency_string: format!(
            "{} -> {}",
            dependency.from_component, dependency.to_component
        ),
    }
}

fn generate_folder_depth_components(
    root: &Path,
    files: &[FileRecord],
    scope_paths: &BTreeSet<String>,
    depth: usize,
    min_component_count: usize,
) -> Vec<NamedSourceCodeAspectConfig> {
    let mut components = Vec::new();

    for current_depth in usize::max(1, depth)..=MAX_COMPONENT_SEARCH_DEPTH {
        components = components_based_on_folder_depth(root, files, scope_paths, current_depth);
        if components.len() >= min_component_count {
            break;
        }
    }

    components
}

fn components_based_on_folder_depth(
    root: &Path,
    files: &[FileRecord],
    scope_paths: &BTreeSet<String>,
    depth: usize,
) -> Vec<NamedSourceCodeAspectConfig> {
    let paths = unique_component_paths(files, scope_paths, depth);
    let greatest_common_prefix = greatest_common_prefix(&paths);
    let root_text = normalized_export_path(root);

    paths
        .into_iter()
        .map(|path| {
            let mut aspect_name = path.clone();
            if aspect_name != greatest_common_prefix {
                aspect_name = aspect_name[greatest_common_prefix.len()..].to_string();
            }
            if aspect_name.is_empty() {
                aspect_name = String::from("ROOT");
            }

            let mut source_file_filters = vec![SourceFileFilterConfig {
                path_pattern: format!("{root_text}/{path}/.*").replace("//", "/"),
                content_pattern: String::new(),
                exception: false,
                note: String::new(),
            }];

            for other_path in unique_component_paths(files, scope_paths, depth) {
                if other_path != path && other_path.starts_with(&path) {
                    source_file_filters.push(SourceFileFilterConfig {
                        path_pattern: format!("{root_text}/{other_path}/.*").replace("//", "/"),
                        content_pattern: String::new(),
                        exception: true,
                        note: String::new(),
                    });
                }
            }

            NamedSourceCodeAspectConfig {
                name: aspect_name,
                source_file_filters,
                files: Vec::new(),
            }
        })
        .collect()
}

fn normalized_export_path(path: &Path) -> String {
    let path_text = path.to_string_lossy().into_owned();
    path_text
        .strip_prefix(r"\\?\")
        .unwrap_or(&path_text)
        .to_string()
}

fn unique_component_paths(
    files: &[FileRecord],
    scope_paths: &BTreeSet<String>,
    depth: usize,
) -> Vec<String> {
    let mut paths = Vec::new();

    for file in files
        .iter()
        .filter(|file| scope_paths.contains(&file.relative_path))
    {
        let component_path = folder_based_component_name(&file.relative_path, depth);
        if !paths.contains(&component_path) {
            paths.push(component_path);
        }
    }

    paths
}

fn folder_based_component_name(relative_path: &str, depth: usize) -> String {
    let segments = relative_path
        .replace('\\', "/")
        .split('/')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut path = String::new();

    for segment in segments
        .iter()
        .take(usize::min(depth, segments.len().saturating_sub(1)))
    {
        path.push_str(segment);
        path.push('/');
    }

    if path.len() > 1 {
        path.truncate(path.len() - 1);
    }

    path
}

fn greatest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }

    let min_length = strings.iter().map(String::len).min().unwrap_or_default();
    for index in 0..min_length {
        let character = strings[0].as_bytes()[index];
        if strings
            .iter()
            .any(|string| string.as_bytes()[index] != character)
        {
            return whole_folder_prefix(&strings[0][..index]);
        }
    }

    whole_folder_prefix(&strings[0][..min_length])
}

fn whole_folder_prefix(prefix: &str) -> String {
    if !prefix.contains('/') && !prefix.contains('\\') {
        return String::new();
    }

    if !prefix.ends_with('/') && !prefix.ends_with('\\') {
        let last_forward = prefix.rfind('/');
        let last_backward = prefix.rfind('\\');
        let last_separator = last_forward.max(last_backward).unwrap_or_default();
        return prefix[..=last_separator].to_string();
    }

    prefix.to_string()
}

#[derive(Debug, Clone)]
struct ConcernGroupMatch {
    key: String,
    concerns: Vec<ConcernMatch>,
}

#[derive(Debug, Clone)]
struct ConcernMatch {
    name: String,
    paths: BTreeSet<String>,
    number_of_regex_line_matches: usize,
    label_in_file_export: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisAspectExport {
    name: Option<String>,
    files_count: usize,
    lines_of_code: usize,
    number_of_regex_line_matches: usize,
    file_count_per_extension: Vec<NumericMetricExport>,
    lines_of_code_per_extension: Vec<NumericMetricExport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConcernsGroupExport {
    key: String,
    concerns: Vec<AnalysisAspectExport>,
    concerns_group: Option<Value>,
    lines_of_code_per_concern: Vec<NumericMetricExport>,
    file_count_per_concern: Vec<NumericMetricExport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FoundTagExport {
    tag_rule: TagRuleConfig,
    evidence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricEntry {
    id: String,
    value: Value,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RiskDistributionExport {
    key: String,
    low_risk_threshold: usize,
    medium_risk_threshold: usize,
    high_risk_threshold: usize,
    very_high_risk_threshold: usize,
    negligible_risk_value: usize,
    low_risk_value: usize,
    medium_risk_value: usize,
    high_risk_value: usize,
    very_high_risk_value: usize,
    low_risk_count: usize,
    negligible_risk_count: usize,
    medium_risk_count: usize,
    high_risk_count: usize,
    very_high_risk_count: usize,
    negligible_risk_label: String,
    low_risk_label: String,
    medium_risk_label: String,
    high_risk_label: String,
    very_high_risk_label: String,
    value_unit: String,
    count_unit: String,
    negligible_risk_percentage: f64,
    total_value: usize,
    total_count: usize,
    very_high_risk_percentage: f64,
    high_risk_percentage: f64,
    medium_risk_percentage: f64,
    low_risk_percentage: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileSizeDistributionPerLogicalDecompositionExport {
    name: String,
    file_size_distribution_per_component: Vec<RiskDistributionExport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitAnalysisExport {
    short_name: String,
    long_name: String,
    source_file: SourceFileExport,
    start_line: usize,
    end_line: usize,
    lines_of_code: usize,
    mc_cabe_index: usize,
    number_of_parameters: usize,
    number_of_literals: usize,
    number_of_statements: usize,
    number_of_expressions: usize,
    children: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FilesAnalysisResultsExport {
    overall_file_size_distribution: RiskDistributionExport,
    file_size_distribution_per_extension: Vec<RiskDistributionExport>,
    file_size_distribution_per_logical_decomposition:
        Vec<FileSizeDistributionPerLogicalDecompositionExport>,
    longest_files: Vec<SourceFileExport>,
    files_with_most_units: Vec<SourceFileExport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitsAnalysisResultsExport {
    total_number_of_units: usize,
    lines_of_code_in_units: usize,
    unit_size_risk_distribution: RiskDistributionExport,
    conditional_complexity_risk_distribution: RiskDistributionExport,
    unit_size_risk_distribution_per_extension: Vec<RiskDistributionExport>,
    unit_size_risk_distribution_per_component: Vec<Vec<RiskDistributionExport>>,
    longest_units: Vec<UnitAnalysisExport>,
    conditional_complexity_risk_distribution_per_extension: Vec<RiskDistributionExport>,
    conditional_complexity_risk_distribution_per_component: Vec<Vec<RiskDistributionExport>>,
    most_complex_units: Vec<UnitAnalysisExport>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicationMetricExport {
    key: String,
    number_of_duplicates: usize,
    cleaned_lines_of_code: usize,
    duplicated_lines_of_code: usize,
    number_of_files_with_duplicates: usize,
    duplication_percentage: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicationAnalysisResultsExport {
    overall_duplication: DuplicationMetricExport,
    duplication_per_component: Vec<Vec<DuplicationMetricExport>>,
    duplication_per_concern: Vec<DuplicationMetricExport>,
    duplication_per_extension: Vec<DuplicationMetricExport>,
    longest_duplicates: Vec<Value>,
    most_frequent_duplicates: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicationSidecarExport {
    title: String,
    timestamp: String,
    duplicates: Vec<Value>,
    overall_duplication: Option<DuplicationMetricExport>,
}

#[derive(Debug, Clone)]
struct AnalysisResultsBundle {
    main_aspect: AnalysisAspectExport,
    test_aspect: AnalysisAspectExport,
    generated_aspect: AnalysisAspectExport,
    build_and_deploy_aspect: AnalysisAspectExport,
    other_aspect: AnalysisAspectExport,
    files_analysis: FilesAnalysisResultsExport,
    units_analysis: UnitsAnalysisResultsExport,
    duplication_analysis: DuplicationAnalysisResultsExport,
    metrics: Vec<MetricEntry>,
    controls: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessingTimeExport {
    processing: String,
    start_ms: u64,
    end_ms: u64,
    duration_ms: u64,
}

fn build_analysis_results_bundle(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    scopes: &ScopeFiles,
    config: &SokratesConfig,
    main_units: &[Unit],
    main_dependencies: &[Dependency],
    concerns_analysis: &[ConcernsGroupExport],
    logical_decompositions: &[LogicalDecompositionExport],
    total_analysis_time_ms: u64,
) -> AnalysisResultsBundle {
    let main_aspect = build_aspect_analysis_result(files, contexts, &scopes.main, None, 0);
    let test_aspect = build_aspect_analysis_result(files, contexts, &scopes.test, None, 0);
    let generated_aspect =
        build_aspect_analysis_result(files, contexts, &scopes.generated, None, 0);
    let build_and_deploy_aspect =
        build_aspect_analysis_result(files, contexts, &scopes.build_and_deployment, None, 0);
    let other_aspect = build_aspect_analysis_result(files, contexts, &scopes.other, None, 0);
    let files_analysis = build_files_analysis_results(
        files,
        contexts,
        &scopes.main,
        config,
        logical_decompositions,
    );
    let units_analysis = build_units_analysis_results(
        files,
        contexts,
        scopes,
        config,
        logical_decompositions,
        main_units,
    );
    let duplication_analysis = build_duplication_analysis_results(
        files,
        contexts,
        scopes,
        config,
        logical_decompositions,
        main_aspect.lines_of_code,
    );
    let metrics = build_metrics_list(
        files,
        &main_aspect,
        &test_aspect,
        &generated_aspect,
        &build_and_deploy_aspect,
        &other_aspect,
        concerns_analysis,
        logical_decompositions,
        &files_analysis,
        &units_analysis,
        &duplication_analysis,
        main_dependencies,
        total_analysis_time_ms,
    );
    let controls = build_controls(&config.goals_and_controls, &metrics);

    AnalysisResultsBundle {
        main_aspect,
        test_aspect,
        generated_aspect,
        build_and_deploy_aspect,
        other_aspect,
        files_analysis,
        units_analysis,
        duplication_analysis,
        metrics,
        controls,
    }
}

fn build_analysis_results_export(
    files: &[FileRecord],
    config: &SokratesConfig,
    metadata: &sokrates_ir::Metadata,
    total_number_of_files_in_scope: usize,
    concerns_analysis: &[ConcernsGroupExport],
    logical_decompositions: &[LogicalDecompositionExport],
    bundle: &AnalysisResultsBundle,
    analysis_start_time_ms: u64,
) -> Value {
    json!({
        "metadata": export_metadata(metadata),
        "metricsList": {
            "metrics": bundle.metrics,
        },
        "controlResults": bundle.controls,
        "totalNumberOfFilesInScope": total_number_of_files_in_scope,
        "mainAspectAnalysisResults": bundle.main_aspect,
        "testAspectAnalysisResults": bundle.test_aspect,
        "generatedAspectAnalysisResults": bundle.generated_aspect,
        "buildAndDeployAspectAnalysisResults": bundle.build_and_deploy_aspect,
        "otherAspectAnalysisResults": bundle.other_aspect,
        "logicalDecompositionsAnalysisResults": logical_decompositions,
        "concernsAnalysisResults": concerns_analysis,
        "foundTags": build_found_tags(files, &config.tag_rules),
        "filesAnalysisResults": bundle.files_analysis,
        "filesHistoryAnalysisResults": build_files_history_analysis_results_export(),
        "unitsAnalysisResults": bundle.units_analysis,
        "duplicationAnalysisResults": bundle.duplication_analysis,
        "contributorsAnalysisResults": build_contributors_analysis_results_export(),
        "numberOfExcludedFiles": 0,
        "excludedExtensions": json!({}),
        "analysisStartTimeMs": analysis_start_time_ms,
        "maxFileCount": max_aspect_file_count([
            &bundle.main_aspect,
            &bundle.test_aspect,
            &bundle.generated_aspect,
            &bundle.build_and_deploy_aspect,
            &bundle.other_aspect,
        ]),
        "maxLinesOfCode": max_aspect_lines_of_code([
            &bundle.main_aspect,
            &bundle.test_aspect,
            &bundle.generated_aspect,
            &bundle.build_and_deploy_aspect,
            &bundle.other_aspect,
        ]),
    })
}

fn build_contributors_export() -> Vec<Value> {
    Vec::new()
}

fn build_duplicates_export() -> DuplicationSidecarExport {
    DuplicationSidecarExport {
        title: String::from("Duplication"),
        timestamp: current_local_timestamp(),
        duplicates: Vec::new(),
        overall_duplication: None,
    }
}

fn write_java_text_and_support_artifacts(
    output_dir: &Path,
    config: &SokratesConfig,
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    scopes: &ScopeFiles,
    concern_groups: &[ConcernGroupMatch],
    logical_decompositions: &[LogicalDecompositionExport],
    main_units: &[Unit],
    main_dependencies: &[Dependency],
    bundle: &AnalysisResultsBundle,
    analysis_time_ms: u64,
    export_started_at: u64,
    export_duration_ms: u64,
) -> Result<()> {
    let text_dir = output_dir.join("text");
    let zip_dir = output_dir.join("zips");
    let _extra_analysis_dir = output_dir.join("extra_analysis");

    if config.analysis.save_source_files {
        write_json(
            &output_dir.join("mainFilesPaths.json"),
            &sorted_paths(&scopes.main),
        )?;
        write_json(
            &output_dir.join("testFilesPaths.json"),
            &sorted_paths(&scopes.test),
        )?;
        write_json(
            &output_dir.join("generatedFilesPaths.json"),
            &sorted_paths(&scopes.generated),
        )?;
        write_json(
            &output_dir.join("buildAndDeploymentFilesPaths.json"),
            &sorted_paths(&scopes.build_and_deployment),
        )?;
        write_json(
            &output_dir.join("otherFilesPaths.json"),
            &sorted_paths(&scopes.other),
        )?;
    }

    write_text(
        &output_dir.join("executionTimes.txt"),
        &build_execution_times_text(&build_execution_times(
            export_started_at,
            export_duration_ms,
        )),
    )?;
    write_json(
        &output_dir.join("executionTimes.json"),
        &build_execution_times(export_started_at, export_duration_ms),
    )?;

    write_text(
        &text_dir.join(aspect_file_list_name("main", "", "")),
        &build_aspect_file_list_text(files, contexts, &scopes.main),
    )?;
    write_text(
        &text_dir.join(aspect_file_list_name("test", "", "")),
        &build_aspect_file_list_text(files, contexts, &scopes.test),
    )?;
    write_text(
        &text_dir.join(aspect_file_list_name("generated", "", "")),
        &build_aspect_file_list_text(files, contexts, &scopes.generated),
    )?;
    write_text(
        &text_dir.join(aspect_file_list_name("build and deployment", "", "")),
        &build_aspect_file_list_text(files, contexts, &scopes.build_and_deployment),
    )?;
    write_text(
        &text_dir.join(aspect_file_list_name("other", "", "")),
        &build_aspect_file_list_text(files, contexts, &scopes.other),
    )?;

    for logical_decomposition in logical_decompositions {
        let scope_paths = scope_paths(scopes, &logical_decomposition.logical_decomposition.scope);
        let prefix = component_file_prefix(&logical_decomposition.key);
        for component in &logical_decomposition.components {
            let component_paths = component_selected_paths(
                files,
                scope_paths,
                &logical_decomposition.key,
                &component.name,
            );
            write_text(
                &text_dir.join(aspect_file_list_name(&component.name, &prefix, "")),
                &build_aspect_file_list_text(files, contexts, &component_paths),
            )?;
        }
        write_text(
            &text_dir.join(format!(
                "{}.txt",
                dependencies_file_name_prefix("", "", &logical_decomposition.key)
            )),
            &build_dependencies_text(logical_decomposition, "", ""),
        )?;
        for dependency in &logical_decomposition.component_dependencies {
            write_text(
                &text_dir.join(format!(
                    "{}.txt",
                    dependencies_file_name_prefix(
                        &dependency.from_component,
                        &dependency.to_component,
                        &logical_decomposition.key,
                    )
                )),
                &build_dependencies_text(
                    logical_decomposition,
                    &dependency.from_component,
                    &dependency.to_component,
                ),
            )?;
        }
    }

    for group in concern_groups {
        let prefix = concern_file_prefix(&group.key);
        for concern in &group.concerns {
            write_text(
                &text_dir.join(aspect_file_list_name(&concern.name, &prefix, "")),
                &build_aspect_file_list_text(files, contexts, &concern.paths),
            )?;
        }
    }

    let main_files = selected_file_records(files, contexts, &scopes.main);
    write_text(
        &text_dir.join("mainFiles.txt"),
        &build_main_files_text(&main_files),
    )?;
    write_text(
        &text_dir.join("mainFilesWithHistory.txt"),
        &build_main_files_with_history_text(&main_files),
    )?;
    write_text(
        &text_dir.join("mainFilesWithoutHistory.txt"),
        &build_main_files_without_history_text(&main_files),
    )?;
    write_text(
        &text_dir.join("contributors.txt"),
        &build_contributors_text(),
    )?;
    write_text(
        &text_dir.join("controls.txt"),
        &build_controls_text(&config.goals_and_controls, &bundle.metrics),
    )?;
    write_text(
        &text_dir.join("metrics.txt"),
        &build_metrics_text(&bundle.metrics),
    )?;
    write_text(
        &text_dir.join("metrics_trend.txt"),
        &build_metrics_trend_text(&bundle.metrics, None, None)?,
    )?;
    write_text(
        &text_dir.join("metrics_trend_loc_per_extension.txt"),
        &build_metrics_trend_text(&bundle.metrics, Some("LINES_OF_CODE_MAIN_.*"), None)?,
    )?;
    write_text(
        &text_dir.join("metrics_trend_loc_duplication.txt"),
        &build_metrics_trend_text(
            &bundle.metrics,
            Some("(DUPLICATION_NUMBER_OF_CLEANED_LINES|DUPLICATION_NUMBER_OF_DUPLICATED_LINES)"),
            None,
        )?,
    )?;
    write_text(
        &text_dir.join("metrics_trend_unit_size_loc.txt"),
        &build_metrics_trend_text(&bundle.metrics, Some("UNIT_SIZE_DISTRIBUTION_.*_LOC"), None)?,
    )?;
    write_text(
        &text_dir.join("metrics_trend_conditional_complexity_loc.txt"),
        &build_metrics_trend_text(&bundle.metrics, Some("CONDITIONAL_COMPLEXITY_.*_LOC"), None)?,
    )?;
    write_text(
        &text_dir.join("metrics_trend_loc_logical_decompositions.txt"),
        &build_metrics_trend_text(
            &bundle.metrics,
            Some("LINES_OF_CODE_DECOMPOSITION_.*"),
            Some(".*_EXT_.*"),
        )?,
    )?;
    write_text(
        &text_dir.join("metrics_trend_loc_file_size.txt"),
        &build_metrics_trend_text(&bundle.metrics, Some("FILE_SIZE_.*"), Some(".*_EXT_.*"))?,
    )?;
    write_text(&text_dir.join("units.txt"), &build_units_text(main_units))?;
    write_text(&text_dir.join("duplicates.txt"), "")?;
    write_text(&text_dir.join("unit_duplicates.txt"), "")?;
    write_text(&text_dir.join("excluded_files_ignored_extensions.txt"), "")?;
    write_text(&text_dir.join("excluded_files_ignored_rules.txt"), "")?;
    write_text(
        &text_dir.join("temporal_dependencies.txt"),
        &build_temporal_dependencies_text(),
    )?;
    write_text(
        &text_dir.join("temporal_dependencies_different_folders.txt"),
        &build_temporal_dependencies_text(),
    )?;
    write_text(
        &text_dir.join("temporal_dependencies_30_days.txt"),
        &build_temporal_dependencies_text(),
    )?;
    write_text(
        &text_dir.join("temporal_dependencies_different_folders_30_days.txt"),
        &build_temporal_dependencies_text(),
    )?;
    write_text(
        &text_dir.join("textualSummary.txt"),
        &build_textual_summary(
            files,
            bundle,
            logical_decompositions,
            concern_groups,
            main_dependencies,
            analysis_time_ms,
        ),
    )?;

    write_all_files_zip(&zip_dir.join("all_files.zip"), &text_dir)?;

    Ok(())
}

fn sorted_paths(paths: &BTreeSet<String>) -> Vec<String> {
    paths.iter().cloned().collect()
}

fn write_text(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

fn build_execution_times(
    export_started_at: u64,
    export_duration_ms: u64,
) -> Vec<ProcessingTimeExport> {
    vec![
        ProcessingTimeExport {
            processing: String::from("everything"),
            start_ms: export_started_at,
            end_ms: 0,
            duration_ms: export_duration_ms,
        },
        ProcessingTimeExport {
            processing: String::from("saving data"),
            start_ms: export_started_at,
            end_ms: export_started_at.saturating_add(export_duration_ms),
            duration_ms: export_duration_ms,
        },
    ]
}

fn build_execution_times_text(entries: &[ProcessingTimeExport]) -> String {
    let total = entries
        .first()
        .map(|entry| entry.duration_ms)
        .unwrap_or_default()
        .max(1);
    let mut content = String::new();

    for entry in entries {
        let seconds = entry.duration_ms as f64 / 1000.0;
        let percentage = 100.0 * entry.duration_ms as f64 / total as f64;
        content.push_str(&format!(
            "{}s => {} ({}%)\n",
            trim_double(seconds),
            entry.processing,
            formatted_percentage_short(percentage, "0")
        ));
    }

    content
}

fn build_aspect_file_list_text(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    selected_paths: &BTreeSet<String>,
) -> String {
    let mut selected = selected_file_records(files, contexts, selected_paths);
    selected.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut content = String::from("Path\tLines of Code\n");
    for file in selected {
        content.push_str(&file.relative_path);
        content.push('\t');
        content.push_str(&file.lines_of_code.to_string());
        content.push('\n');
    }

    content
}

fn aspect_file_list_name(name: &str, prefix: &str, suffix: &str) -> String {
    format!(
        "aspect_{}{}.txt",
        safe_file_system_name(&format!("{prefix}{name}")),
        suffix
    )
}

fn component_file_prefix(logical_decomposition_name: &str) -> String {
    format!("component_{}_", logical_decomposition_name)
}

fn concern_file_prefix(concern_group: &str) -> String {
    format!("concern_{}_", concern_group)
}

fn safe_file_system_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch == '.' || ch.is_ascii_alphanumeric() {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn component_selected_paths(
    files: &[FileRecord],
    scope_paths: &BTreeSet<String>,
    decomposition: &str,
    component: &str,
) -> BTreeSet<String> {
    files
        .iter()
        .filter(|file| scope_paths.contains(&file.relative_path))
        .filter(|file| file_has_component(file, decomposition, component))
        .map(|file| file.relative_path.clone())
        .collect()
}

fn build_main_files_text(main_files: &[FileRecord]) -> String {
    let mut content = String::from("path\t# lines of code\n");
    for file in main_files {
        content.push_str(&file.relative_path);
        content.push('\t');
        content.push_str(&file.lines_of_code.to_string());
        content.push('\n');
    }
    content
}

fn build_main_files_with_history_text(_main_files: &[FileRecord]) -> String {
    String::from(
        "path\t# lines of code\t# active days\tdays since first update\tdays since last update\t# commits\t# contributors\tfirst updated\tlast updated\tfirst contributor\tlast contributor\n",
    )
}

fn build_main_files_without_history_text(main_files: &[FileRecord]) -> String {
    let mut content = String::from("path\t# lines of code\n");
    for file in main_files {
        content.push_str(&file.relative_path);
        content.push('\t');
        content.push_str(&file.lines_of_code.to_string());
        content.push('\t');
        content.push('\n');
    }
    content
}

fn build_contributors_text() -> String {
    String::from(
        "Contributor\t#commits (all time)\t#commits (30 days)\t#commits (90 days)\t#commits (180 days)\t#commits (365 days)\tfirst commit\tlast commit\n",
    )
}

fn build_controls_text(goals: &[MetricsWithGoalConfig], metrics: &[MetricEntry]) -> String {
    let metrics_map = metrics
        .iter()
        .map(|metric| (metric.id.to_ascii_uppercase(), metric))
        .collect::<BTreeMap<_, _>>();
    let mut content = String::new();

    for goal in goals {
        for control in &goal.controls {
            let metric = metrics_map
                .get(&control.metric.to_ascii_uppercase())
                .copied();
            let metric_id_value = metric
                .map(|metric| metric.id.clone())
                .unwrap_or_else(|| metric_id(&control.metric));
            let metric_value = metric
                .map(|metric| metric.value.clone())
                .unwrap_or(Value::Null);
            let status = control_status(control, &metric_value);

            content.push_str("goal: ");
            content.push_str(&goal.goal);
            content.push('\n');
            content.push_str("control metric: ");
            content.push_str(&metric_id_value);
            content.push('\n');
            content.push_str("status: ");
            content.push_str(status);
            content.push('\n');
            content.push_str("desired range: ");
            content.push_str(&range_text_description(&control.desired_range));
            content.push('\n');
            content.push_str("value: ");
            content.push_str(&metric_value_text(&metric_value));
            content.push('\n');
            content.push_str("description: ");
            content.push_str(&control.description);
            content.push_str("\n\n");
        }
    }

    content
}

fn range_text_description(range: &RangeConfig) -> String {
    format!("[{} - {}] ±{}", range.min, range.max, range.tolerance)
}

fn build_metrics_text(metrics: &[MetricEntry]) -> String {
    let mut content = String::new();
    for metric in metrics {
        content.push_str(&metric.id);
        content.push_str(": ");
        content.push_str(&metric_value_text(&metric.value));
        content.push('\n');
    }
    content
}

fn metric_value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Null => String::from("null"),
        Value::Bool(value) => value.to_string(),
        other => other.to_string(),
    }
}

fn build_metrics_trend_text(
    metrics: &[MetricEntry],
    include_regex: Option<&str>,
    exclude_regex: Option<&str>,
) -> Result<String> {
    let mut filtered = Vec::new();

    for metric in metrics {
        if should_include_metric(metric, include_regex, exclude_regex)? {
            filtered.push(metric);
        }
    }

    if filtered.is_empty() {
        return Ok(String::new());
    }

    let mut content = String::from("Metric\t\tCurrent\n");
    for metric in filtered {
        content.push_str(&metric.id);
        content.push_str("\t\t");
        content.push_str(&format_number_for_report(&metric.value));
        content.push('\n');
    }

    Ok(content)
}

fn should_include_metric(
    metric: &MetricEntry,
    include_regex: Option<&str>,
    exclude_regex: Option<&str>,
) -> Result<bool> {
    let include = match include_regex {
        Some(pattern) => matches_entire(pattern, &metric.id)?,
        None => true,
    };
    let exclude = match exclude_regex {
        Some(pattern) => matches_entire(pattern, &metric.id)?,
        None => false,
    };

    Ok(include && !exclude)
}

fn format_number_for_report(value: &Value) -> String {
    let Some(value) = metric_value_as_f64(value) else {
        return String::from("-");
    };

    if value.is_nan() {
        return String::from("-");
    }

    if (value.fract()).abs() < f64::EPSILON {
        return format_grouped_i64(value as i64);
    }

    trim_double(value)
}

fn trim_double(value: f64) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn format_grouped_i64(value: i64) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
    let mut grouped = String::new();

    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }

    let mut grouped = grouped.chars().rev().collect::<String>();
    if negative {
        grouped.insert(0, '-');
    }
    grouped
}

fn build_units_text(units: &[Unit]) -> String {
    let mut content = String::new();

    for (index, unit) in units.iter().enumerate() {
        content.push_str(&format!("id: {}\n", index + 1));
        content.push_str("unit: ");
        content.push_str(&unit.short_name);
        content.push('\n');
        content.push_str("file: ");
        content.push_str(&unit.relative_file_name);
        content.push('\n');
        content.push_str(&format!("start line: {}\n", unit.start_line));
        content.push_str(&format!("end line: {}\n", unit.end_line));
        content.push_str(&format!("size: {} LOC\n", unit.lines_of_code));
        content.push_str(&format!("McCabe index: {}\n", unit.mc_cabe_index));
        content.push_str(&format!(
            "number of parameters: {}\n\n",
            unit.number_of_parameters
        ));
    }

    content
}

fn dependencies_file_name_prefix(
    from_component: &str,
    to_component: &str,
    logical_decomposition_name: &str,
) -> String {
    let mut file_name_prefix = format!(
        "dependencies_{}",
        safe_file_system_name(logical_decomposition_name)
    );
    if !from_component.is_empty() && !to_component.is_empty() {
        file_name_prefix.push('_');
        file_name_prefix.push_str(&safe_file_system_name(&format!(
            "{}_{}",
            from_component, to_component
        )));
    }
    file_name_prefix
}

fn build_dependencies_text(
    logical_decomposition: &LogicalDecompositionExport,
    filter_from: &str,
    filter_to: &str,
) -> String {
    let mut content = String::new();

    for dependency in &logical_decomposition.component_dependencies {
        if (!filter_from.is_empty() || !filter_to.is_empty())
            && (dependency.from_component != filter_from || dependency.to_component != filter_to)
        {
            continue;
        }

        for evidence in &dependency.evidence {
            content.push_str("from: ");
            content.push_str(&dependency.from_component);
            content.push('\n');
            content.push_str("to: ");
            content.push_str(&dependency.to_component);
            content.push_str("\nevidence:\n");
            content.push_str(" - file: \"");
            content.push_str(&evidence.path_from);
            content.push_str("\"\n");
            content.push_str("   contains \"");
            content.push_str(&evidence.evidence);
            content.push_str("\"\n\n");
        }
    }

    content
}

fn build_temporal_dependencies_text() -> String {
    String::from("file 1\tfile 2\t# same commits\t# commits file 1\t# commits file 2\n")
}

fn build_textual_summary(
    files: &[FileRecord],
    bundle: &AnalysisResultsBundle,
    logical_decompositions: &[LogicalDecompositionExport],
    concern_groups: &[ConcernGroupMatch],
    main_dependencies: &[Dependency],
    analysis_time_ms: u64,
) -> String {
    let mut lines = Vec::new();

    lines.push(String::from("Start of analysis"));
    lines.push(format!("Found {} files", files.len()));
    append_aspect_summary(&mut lines, "main", &bundle.main_aspect);
    append_aspect_summary(&mut lines, "test", &bundle.test_aspect);
    append_aspect_summary(&mut lines, "generated", &bundle.generated_aspect);
    append_aspect_summary(
        &mut lines,
        "build and deployment",
        &bundle.build_and_deploy_aspect,
    );
    append_aspect_summary(&mut lines, "other", &bundle.other_aspect);
    lines.push(String::from("Excluded from analyses 0 files"));
    lines.push(String::from("Analysing dependencies..."));

    for logical_decomposition in logical_decompositions {
        for component in &logical_decomposition.components {
            append_aspect_summary(
                &mut lines,
                &format!(
                    "DECOMPOSITION_{}\\{}",
                    logical_decomposition.key, component.name
                ),
                &AnalysisAspectExport {
                    name: Some(component.name.clone()),
                    files_count: component.files_count,
                    lines_of_code: component.lines_of_code,
                    number_of_regex_line_matches: component.number_of_regex_line_matches,
                    file_count_per_extension: component.file_count_per_extension.clone(),
                    lines_of_code_per_extension: component.lines_of_code_per_extension.clone(),
                },
            );
        }
        lines.push(format!(
            "  - \"{}\", found {} dependencies ({} component dependencies)",
            logical_decomposition.key,
            dependency_links_count(logical_decomposition, main_dependencies),
            logical_decomposition.component_dependencies.len()
        ));
    }

    for group in concern_groups {
        for concern in &group.concerns {
            append_aspect_summary(
                &mut lines,
                &format!("CONCERN_{}\\{}", group.key, concern.name),
                &build_aspect_analysis_result_from_paths(files, concern),
            );
        }
    }

    lines.push(String::from("Analysing units..."));
    lines.push(String::from("Basic unit metrics..."));
    lines.push(format!(
        "  - found {} units ({} lines of code in units)",
        bundle.units_analysis.total_number_of_units, bundle.units_analysis.lines_of_code_in_units
    ));
    lines.push(format!(
        "Unit size {}",
        risk_distribution_value_text(&bundle.units_analysis.unit_size_risk_distribution)
    ));
    lines.push(String::from("Unit size per component:"));
    for group in &bundle
        .units_analysis
        .unit_size_risk_distribution_per_component
    {
        for distribution in group {
            lines.push(format!(
                "Unit Size Component {}: {}",
                distribution.key,
                risk_distribution_value_text(distribution)
            ));
        }
    }
    lines.push(String::from("Unit size per extension:"));
    for distribution in &bundle
        .units_analysis
        .unit_size_risk_distribution_per_extension
    {
        lines.push(format!(
            "Unit Size Extension {}: {}",
            distribution.key,
            risk_distribution_value_text(distribution)
        ));
    }
    lines.push(format!(
        "Conditional complexity: {}",
        risk_distribution_value_text(
            &bundle
                .units_analysis
                .conditional_complexity_risk_distribution
        )
    ));
    lines.push(String::from("Conditional complexity per component:"));
    for group in &bundle
        .units_analysis
        .conditional_complexity_risk_distribution_per_component
    {
        for distribution in group {
            lines.push(format!(
                "Conditional Complexity Component {} {}",
                distribution.key,
                risk_distribution_value_text(distribution)
            ));
        }
    }
    lines.push(String::from("Conditional complexity per extension:"));
    for distribution in &bundle
        .units_analysis
        .conditional_complexity_risk_distribution_per_extension
    {
        lines.push(format!(
            "Conditional Complexity Component {}: {}",
            distribution.key,
            risk_distribution_value_text(distribution)
        ));
    }
    lines.push(String::from("Analysing duplication..."));
    lines.push(format!(
        "  - found {} duplicates ({} duplicated lines vs. {} cleaned lines) in {} files",
        bundle
            .duplication_analysis
            .overall_duplication
            .number_of_duplicates,
        bundle
            .duplication_analysis
            .overall_duplication
            .duplicated_lines_of_code,
        bundle
            .duplication_analysis
            .overall_duplication
            .cleaned_lines_of_code,
        bundle
            .duplication_analysis
            .overall_duplication
            .number_of_files_with_duplicates
    ));
    for (logical_decomposition, component_group) in logical_decompositions
        .iter()
        .zip(bundle.duplication_analysis.duplication_per_component.iter())
    {
        lines.push(format!("  - per component:{}", logical_decomposition.key));
        for component in component_group {
            if component.cleaned_lines_of_code > 0 || component.duplicated_lines_of_code > 0 {
                lines.push(format!(
                    "     - \"{}\": {} duplicated lines vs. {} total lines",
                    component.key,
                    component.duplicated_lines_of_code,
                    component.cleaned_lines_of_code
                ));
            }
        }
    }
    lines.push(String::from("  - per extension:"));
    lines.push(format!(
        "Total analysis time: {}s",
        format_analysis_duration_text(analysis_time_ms)
    ));
    lines.push(String::new());

    lines.join("\n")
}

fn build_aspect_analysis_result_from_paths(
    files: &[FileRecord],
    concern: &ConcernMatch,
) -> AnalysisAspectExport {
    let selected = files
        .iter()
        .filter(|file| concern.paths.contains(&file.relative_path))
        .cloned()
        .collect::<Vec<_>>();
    let lines_of_code = selected.iter().map(|file| file.lines_of_code).sum();

    AnalysisAspectExport {
        name: Some(concern.name.clone()),
        files_count: selected.len(),
        lines_of_code,
        number_of_regex_line_matches: concern.number_of_regex_line_matches,
        file_count_per_extension: extension_metrics(&selected, |group| group.len()),
        lines_of_code_per_extension: extension_metrics(&selected, |group| {
            group.iter().map(|file| file.lines_of_code).sum()
        }),
    }
}

fn append_aspect_summary(lines: &mut Vec<String>, name: &str, aspect: &AnalysisAspectExport) {
    lines.push(format!(
        "{} are in the {}'s scope  ({} lines of code)",
        aspect.files_count, name, aspect.lines_of_code
    ));
    for metric in &aspect.file_count_per_extension {
        let loc = aspect
            .lines_of_code_per_extension
            .iter()
            .find(|candidate| candidate.name == metric.name)
            .map(|candidate| candidate.value)
            .unwrap_or_default();
        lines.push(format!(
            "{}: {} files, ({} lines of code)",
            metric.name, metric.value, loc
        ));
    }
}

fn dependency_links_count(
    logical_decomposition: &LogicalDecompositionExport,
    main_dependencies: &[Dependency],
) -> usize {
    let uses_built_in_dependency_finders = logical_decomposition
        .logical_decomposition
        .dependencies_finder
        .get("useBuiltInDependencyFinders")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    if uses_built_in_dependency_finders {
        main_dependencies.len()
    } else {
        0
    }
}

fn risk_distribution_value_text(distribution: &RiskDistributionExport) -> String {
    format!(
        "{} / {} / {} / {} / {}",
        distribution.negligible_risk_value,
        distribution.low_risk_value,
        distribution.medium_risk_value,
        distribution.high_risk_value,
        distribution.very_high_risk_value
    )
}

fn format_analysis_duration_text(duration_ms: u64) -> String {
    let rounded = (duration_ms / 10) as f64 * 0.01;
    let mut text = format!("{rounded:.2}");
    if text.starts_with('0') {
        text.remove(0);
    }
    text
}

fn formatted_percentage_short(value: f64, zero_text: &str) -> String {
    if value < 0.0000000000000000000001 {
        zero_text.to_string()
    } else if value < 1.0 {
        String::from("<1")
    } else {
        format!("{}", value as i64)
    }
}

fn write_all_files_zip(path: &Path, text_dir: &Path) -> Result<()> {
    let file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for entry_name in [
        "aspect_main.txt",
        "aspect_test.txt",
        "aspect_generated.txt",
        "aspect_build_and_deployment.txt",
        "aspect_other.txt",
    ] {
        let content = fs::read(text_dir.join(entry_name))
            .with_context(|| format!("read {}", text_dir.join(entry_name).display()))?;
        zip.start_file(entry_name, options)
            .with_context(|| format!("add {entry_name} to {}", path.display()))?;
        zip.write_all(&content)
            .with_context(|| format!("write {entry_name} to {}", path.display()))?;
    }

    zip.finish()
        .with_context(|| format!("finish {}", path.display()))?;
    Ok(())
}

fn export_metadata(metadata: &sokrates_ir::Metadata) -> Value {
    json!({
        "name": metadata.name,
        "description": metadata.description,
        "tooltip": "",
        "logoLink": "",
        "links": [],
    })
}

fn max_aspect_file_count<'a>(aspects: impl IntoIterator<Item = &'a AnalysisAspectExport>) -> usize {
    aspects
        .into_iter()
        .map(|aspect| aspect.files_count)
        .max()
        .unwrap_or_default()
}

fn max_aspect_lines_of_code<'a>(
    aspects: impl IntoIterator<Item = &'a AnalysisAspectExport>,
) -> usize {
    aspects
        .into_iter()
        .map(|aspect| aspect.lines_of_code)
        .max()
        .unwrap_or_default()
}

fn build_files_history_analysis_results_export() -> Value {
    json!({
        "overallFileLastModifiedDistribution": Value::Null,
        "overallFileFirstModifiedDistribution": Value::Null,
        "overallFileChangeDistribution": Value::Null,
        "overallContributorsCountDistribution": Value::Null,
        "filesWithoutCommitHistoryCount": 0,
        "filesWithoutCommitHistoryLinesOfCode": 0,
        "changeDistributionPerExtension": [],
        "lastModifiedDistributionPerExtension": [],
        "firstModifiedDistributionPerExtension": [],
        "changeDistributionPerLogicalDecomposition": [],
        "firstModifiedDistributionPerLogicalDecomposition": [],
        "lastModifiedDistributionPerLogicalDecomposition": [],
        "oldestFiles": [],
        "youngestFiles": [],
        "mostRecentlyChangedFiles": [],
        "mostPreviouslyChangedFiles": [],
        "mostChangedFiles": [],
        "filesWithMostContributors": [],
        "filesWithLeastContributors": [],
        "firstDate": "",
        "latestDate": "",
        "daysBetweenFirstAndLastDate": 0,
        "weeks": 0,
        "estimatedWorkindDays": 0,
        "activeDays": 0,
        "ageInDays": 0,
        "historyIndexed": false,
        "historySource": "",
        "historyFileCount": 0,
        "historyPerExtensionPerYear": [],
    })
}

fn build_contributors_analysis_results_export() -> Value {
    json!({
        "latestCommitDate": "",
        "contributors": [],
        "contributorsPerYear": [],
        "contributorsPerMonth": [],
        "contributorsPerDay": [],
        "contributorsPerWeek": [],
        "commitsPerExtensions": [],
    })
}

fn build_aspect_analysis_result(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    selected_paths: &BTreeSet<String>,
    name: Option<String>,
    number_of_regex_line_matches: usize,
) -> AnalysisAspectExport {
    let selected = selected_file_records(files, contexts, selected_paths);
    let lines_of_code = selected.iter().map(|file| file.lines_of_code).sum();

    AnalysisAspectExport {
        name,
        files_count: selected.len(),
        lines_of_code,
        number_of_regex_line_matches,
        file_count_per_extension: extension_metrics(&selected, |group| group.len()),
        lines_of_code_per_extension: extension_metrics(&selected, |group| {
            group.iter().map(|file| file.lines_of_code).sum()
        }),
    }
}

fn selected_file_records(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    selected_paths: &BTreeSet<String>,
) -> Vec<FileRecord> {
    files
        .iter()
        .filter(|file| selected_paths.contains(&file.relative_path))
        .filter_map(|file| contexts.get(&file.relative_path))
        .map(|context| context.file.clone())
        .collect()
}

fn analyze_concern_groups(
    contexts: &BTreeMap<String, ExportFileContext>,
    main_paths: &BTreeSet<String>,
    config: &SokratesConfig,
) -> Result<Vec<ConcernGroupMatch>> {
    let mut groups = Vec::new();

    for group in &config.concern_groups {
        let mut concerns = Vec::new();

        for concern in &group.concerns {
            let (paths, number_of_regex_line_matches) =
                evaluate_concern(contexts, main_paths, concern)?;
            concerns.push(ConcernMatch {
                name: concern.name.clone(),
                paths,
                number_of_regex_line_matches,
                label_in_file_export: true,
            });
        }

        let mut unclassified = BTreeSet::new();
        let mut multiple_classifications = BTreeSet::new();

        for path in main_paths {
            let count = concerns
                .iter()
                .filter(|concern| concern.paths.contains(path))
                .count();
            if count == 0 {
                unclassified.insert(path.clone());
            } else if count > 1 {
                multiple_classifications.insert(path.clone());
            }
        }

        if !unclassified.is_empty() {
            concerns.push(ConcernMatch {
                name: String::from(UNCLASSIFIED_FILES),
                paths: unclassified,
                number_of_regex_line_matches: 0,
                label_in_file_export: true,
            });
        }

        if !multiple_classifications.is_empty() {
            concerns.push(ConcernMatch {
                name: String::from(MULTIPLE_CLASSIFICATIONS),
                paths: multiple_classifications,
                number_of_regex_line_matches: 0,
                label_in_file_export: false,
            });
        }

        groups.push(ConcernGroupMatch {
            key: group.name.clone(),
            concerns,
        });
    }

    Ok(groups)
}

fn evaluate_concern(
    contexts: &BTreeMap<String, ExportFileContext>,
    main_paths: &BTreeSet<String>,
    concern: &ConcernConfig,
) -> Result<(BTreeSet<String>, usize)> {
    let mut matches = BTreeSet::new();
    let mut number_of_regex_line_matches = 0;

    for path in main_paths {
        let context = contexts
            .get(path)
            .with_context(|| format!("missing file context for {path}"))?;
        let mut included = concern.files.iter().any(|file| file == path);
        let mut excluded = false;

        for filter in &concern.source_file_filters {
            if filter_matches(filter, context)? {
                if filter.exception {
                    excluded = true;
                } else {
                    included = true;
                    number_of_regex_line_matches += matching_line_count(filter, context)?;
                }
            }
        }

        if included && !excluded {
            matches.insert(path.clone());
        }
    }

    Ok((matches, number_of_regex_line_matches))
}

fn matching_line_count(
    filter: &SourceFileFilterConfig,
    context: &ExportFileContext,
) -> Result<usize> {
    if filter.content_pattern.trim().is_empty()
        || !path_matches(&filter.path_pattern, &context.absolute_path)?
    {
        return Ok(0);
    }

    let mut count = 0;
    for line in &context.lines {
        if matches_entire(&filter.content_pattern, line)? {
            count += 1;
        }
    }

    Ok(count)
}

fn concern_labels_from_groups(
    main_paths: &BTreeSet<String>,
    groups: &[ConcernGroupMatch],
) -> BTreeMap<String, Vec<String>> {
    let mut labels = main_paths
        .iter()
        .map(|path| (path.clone(), Vec::new()))
        .collect::<BTreeMap<_, _>>();

    for path in main_paths {
        let file_labels = labels.entry(path.clone()).or_default();
        for group in groups {
            for concern in &group.concerns {
                if concern.label_in_file_export && concern.paths.contains(path) {
                    file_labels.push(format!("::{}", concern.name));
                }
            }
        }
    }

    labels
}

fn export_concerns_analysis(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    groups: &[ConcernGroupMatch],
) -> Vec<ConcernsGroupExport> {
    groups
        .iter()
        .map(|group| {
            let concerns = group
                .concerns
                .iter()
                .map(|concern| {
                    build_aspect_analysis_result(
                        files,
                        contexts,
                        &concern.paths,
                        Some(concern.name.clone()),
                        concern.number_of_regex_line_matches,
                    )
                })
                .collect::<Vec<_>>();
            let lines_of_code_per_concern = concerns
                .iter()
                .map(|concern| NumericMetricExport {
                    name: concern.name.clone().unwrap_or_default(),
                    value: concern.lines_of_code,
                    description: Vec::new(),
                })
                .collect::<Vec<_>>();
            let file_count_per_concern = concerns
                .iter()
                .map(|concern| NumericMetricExport {
                    name: concern.name.clone().unwrap_or_default(),
                    value: concern.files_count,
                    description: Vec::new(),
                })
                .collect::<Vec<_>>();

            ConcernsGroupExport {
                key: group.key.clone(),
                concerns,
                concerns_group: None,
                lines_of_code_per_concern,
                file_count_per_concern,
            }
        })
        .collect()
}

fn build_found_tags(files: &[FileRecord], tag_rules: &[TagRuleConfig]) -> Vec<FoundTagExport> {
    let paths = files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();

    tag_rules
        .iter()
        .filter_map(|tag_rule| {
            let mut matched_paths = paths
                .iter()
                .filter(|path| tag_rule_matches_path(tag_rule, path))
                .cloned()
                .collect::<Vec<_>>();
            matched_paths.sort();
            if matched_paths.is_empty() {
                return None;
            }

            let mut evidence = matched_paths
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            if matched_paths.len() > 20 {
                evidence.push_str(&format!(
                    "\n...\n(found {} more files)",
                    matched_paths.len() - 20
                ));
            }

            Some(FoundTagExport {
                tag_rule: tag_rule.clone(),
                evidence,
            })
        })
        .collect()
}

fn tag_rule_matches_path(tag_rule: &TagRuleConfig, path: &str) -> bool {
    tag_rule
        .path_patterns
        .iter()
        .any(|pattern| matches_entire(pattern, path).unwrap_or(false))
        && !tag_rule
            .exclude_path_patterns
            .iter()
            .any(|pattern| matches_entire(pattern, path).unwrap_or(false))
}

fn build_files_analysis_results(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    main_paths: &BTreeSet<String>,
    config: &SokratesConfig,
    logical_decompositions: &[LogicalDecompositionExport],
) -> FilesAnalysisResultsExport {
    let main_files = selected_file_records(files, contexts, main_paths);
    let overall_file_size_distribution = build_risk_distribution(
        "",
        &config.analysis.file_size_thresholds,
        main_files
            .iter()
            .map(|file| (file.lines_of_code, file.lines_of_code)),
    );
    let file_size_distribution_per_extension = build_file_size_distributions_per_extension(
        &main_files,
        &config.analysis.file_size_thresholds,
    );
    let file_size_distribution_per_logical_decomposition = logical_decompositions
        .iter()
        .map(
            |logical_decomposition| FileSizeDistributionPerLogicalDecompositionExport {
                name: logical_decomposition.key.clone(),
                file_size_distribution_per_component: logical_decomposition
                    .components
                    .iter()
                    .map(|component| {
                        build_risk_distribution(
                            &component.name,
                            &config.analysis.file_size_thresholds,
                            main_files
                                .iter()
                                .filter(|file| {
                                    file_has_component(
                                        file,
                                        &logical_decomposition.key,
                                        &component.name,
                                    )
                                })
                                .map(|file| (file.lines_of_code, file.lines_of_code)),
                        )
                    })
                    .collect(),
            },
        )
        .collect::<Vec<_>>();
    let mut longest_files = main_files.clone();
    longest_files.sort_by(|left, right| {
        right
            .lines_of_code
            .cmp(&left.lines_of_code)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    longest_files.truncate(50);

    let mut files_with_most_units = main_files
        .iter()
        .filter(|file| file.units_count > 0)
        .cloned()
        .collect::<Vec<_>>();
    files_with_most_units.sort_by(|left, right| {
        right
            .units_count
            .cmp(&left.units_count)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    files_with_most_units.truncate(50);

    FilesAnalysisResultsExport {
        overall_file_size_distribution,
        file_size_distribution_per_extension,
        file_size_distribution_per_logical_decomposition,
        longest_files: longest_files
            .iter()
            .filter_map(|file| contexts.get(&file.relative_path))
            .map(|context| source_file_export_from_context(context, true))
            .collect(),
        files_with_most_units: files_with_most_units
            .iter()
            .filter_map(|file| contexts.get(&file.relative_path))
            .map(|context| source_file_export_from_context(context, true))
            .collect(),
    }
}

fn build_units_analysis_results(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    scopes: &ScopeFiles,
    config: &SokratesConfig,
    logical_decompositions: &[LogicalDecompositionExport],
    main_units: &[Unit],
) -> UnitsAnalysisResultsExport {
    let lines_of_code_in_units = main_units.iter().map(|unit| unit.lines_of_code).sum();
    let unit_size_risk_distribution = build_risk_distribution(
        "",
        &config.analysis.unit_size_thresholds,
        main_units
            .iter()
            .map(|unit| (unit.lines_of_code, unit.lines_of_code)),
    );
    let conditional_complexity_risk_distribution = build_risk_distribution(
        "",
        &config.analysis.conditional_complexity_thresholds,
        main_units
            .iter()
            .map(|unit| (unit.mc_cabe_index, unit.lines_of_code)),
    );
    let unit_size_risk_distribution_per_extension = build_unit_distributions_per_extension(
        main_units,
        contexts,
        &config.analysis.unit_size_thresholds,
        UnitDistributionKind::Size,
    );
    let unit_size_risk_distribution_per_component = build_unit_distributions_per_component(
        main_units,
        scopes,
        &config.analysis.unit_size_thresholds,
        logical_decompositions,
        UnitDistributionKind::Size,
    );
    let conditional_complexity_risk_distribution_per_extension =
        build_unit_distributions_per_extension(
            main_units,
            contexts,
            &config.analysis.conditional_complexity_thresholds,
            UnitDistributionKind::ConditionalComplexity,
        );
    let conditional_complexity_risk_distribution_per_component =
        build_unit_distributions_per_component(
            main_units,
            scopes,
            &config.analysis.conditional_complexity_thresholds,
            logical_decompositions,
            UnitDistributionKind::ConditionalComplexity,
        );
    let longest_units = ordered_units_by_length(main_units)
        .into_iter()
        .take(config.analysis.max_top_list_size)
        .map(|unit| build_unit_analysis_export(unit, contexts))
        .collect();
    let most_complex_units = ordered_units_by_complexity(main_units)
        .into_iter()
        .take(config.analysis.max_top_list_size)
        .map(|unit| build_unit_analysis_export(unit, contexts))
        .collect();
    let _ = files;

    UnitsAnalysisResultsExport {
        total_number_of_units: main_units.len(),
        lines_of_code_in_units,
        unit_size_risk_distribution,
        conditional_complexity_risk_distribution,
        unit_size_risk_distribution_per_extension,
        unit_size_risk_distribution_per_component,
        longest_units,
        conditional_complexity_risk_distribution_per_extension,
        conditional_complexity_risk_distribution_per_component,
        most_complex_units,
    }
}

#[derive(Debug, Clone, Copy)]
enum UnitDistributionKind {
    Size,
    ConditionalComplexity,
}

fn build_unit_distributions_per_extension(
    units: &[Unit],
    contexts: &BTreeMap<String, ExportFileContext>,
    thresholds: &ThresholdsConfig,
    kind: UnitDistributionKind,
) -> Vec<RiskDistributionExport> {
    let mut extensions = Vec::<String>::new();
    for unit in units {
        let extension = contexts
            .get(&unit.relative_file_name)
            .map(|context| context.file.extension.clone())
            .unwrap_or_default();
        if !extensions.contains(&extension) {
            extensions.push(extension);
        }
    }

    extensions
        .into_iter()
        .map(|extension| {
            build_risk_distribution(
                &extension,
                thresholds,
                units
                    .iter()
                    .filter(|unit| {
                        contexts
                            .get(&unit.relative_file_name)
                            .map(|context| context.file.extension.eq_ignore_ascii_case(&extension))
                            .unwrap_or(false)
                    })
                    .map(|unit| match kind {
                        UnitDistributionKind::Size => (unit.lines_of_code, unit.lines_of_code),
                        UnitDistributionKind::ConditionalComplexity => {
                            (unit.mc_cabe_index, unit.lines_of_code)
                        }
                    }),
            )
        })
        .collect()
}

fn build_unit_distributions_per_component(
    units: &[Unit],
    scopes: &ScopeFiles,
    thresholds: &ThresholdsConfig,
    logical_decompositions: &[LogicalDecompositionExport],
    kind: UnitDistributionKind,
) -> Vec<Vec<RiskDistributionExport>> {
    logical_decompositions
        .iter()
        .map(|logical_decomposition| {
            let scope_paths =
                scope_paths(scopes, &logical_decomposition.logical_decomposition.scope);
            logical_decomposition
                .components
                .iter()
                .map(|component| {
                    build_risk_distribution(
                        &component.name,
                        thresholds,
                        units
                            .iter()
                            .filter(|unit| scope_paths.contains(&unit.relative_file_name))
                            .filter(|unit| {
                                unit_has_component(
                                    unit,
                                    &logical_decomposition.key,
                                    &component.name,
                                )
                            })
                            .map(|unit| match kind {
                                UnitDistributionKind::Size => {
                                    (unit.lines_of_code, unit.lines_of_code)
                                }
                                UnitDistributionKind::ConditionalComplexity => {
                                    (unit.mc_cabe_index, unit.lines_of_code)
                                }
                            }),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn ordered_units_by_length(units: &[Unit]) -> Vec<Unit> {
    let mut ordered = units.to_vec();
    ordered.sort_by(|left, right| {
        right
            .lines_of_code
            .cmp(&left.lines_of_code)
            .then_with(|| left.relative_file_name.cmp(&right.relative_file_name))
            .then_with(|| left.start_line.cmp(&right.start_line))
    });
    ordered
}

fn ordered_units_by_complexity(units: &[Unit]) -> Vec<Unit> {
    let mut ordered = units.to_vec();
    ordered.sort_by(|left, right| {
        right
            .mc_cabe_index
            .cmp(&left.mc_cabe_index)
            .then_with(|| left.relative_file_name.cmp(&right.relative_file_name))
            .then_with(|| left.start_line.cmp(&right.start_line))
    });
    ordered
}

fn build_unit_analysis_export(
    unit: Unit,
    contexts: &BTreeMap<String, ExportFileContext>,
) -> UnitAnalysisExport {
    let source_file = contexts
        .get(&unit.relative_file_name)
        .map(|context| source_file_export_from_context(context, true))
        .unwrap_or(SourceFileExport {
            relative_path: unit.relative_file_name.clone(),
            extension: String::new(),
            lines_of_code: unit.file_lines_count,
            units_count: 0,
            units_mc_cabe_index_sum: 0,
            lines_of_code_in_units: 0,
        });

    UnitAnalysisExport {
        short_name: unit.short_name.clone(),
        long_name: unit.long_name.clone(),
        source_file,
        start_line: unit.start_line,
        end_line: unit.end_line,
        lines_of_code: unit.lines_of_code,
        mc_cabe_index: unit.mc_cabe_index,
        number_of_parameters: unit.number_of_parameters,
        number_of_literals: unit.number_of_literals,
        number_of_statements: unit.number_of_statements,
        number_of_expressions: unit.number_of_expressions,
        children: Vec::new(),
    }
}

fn build_duplication_analysis_results(
    files: &[FileRecord],
    contexts: &BTreeMap<String, ExportFileContext>,
    scopes: &ScopeFiles,
    config: &SokratesConfig,
    logical_decompositions: &[LogicalDecompositionExport],
    main_lines_of_code: usize,
) -> DuplicationAnalysisResultsExport {
    let skip_duplication = config.analysis.skip_duplication
        || main_lines_of_code > config.analysis.loc_duplication_threshold;

    if skip_duplication {
        return DuplicationAnalysisResultsExport {
            overall_duplication: duplication_metric("system", 0, 0, 0, 0),
            duplication_per_component: Vec::new(),
            duplication_per_concern: Vec::new(),
            duplication_per_extension: Vec::new(),
            longest_duplicates: Vec::new(),
            most_frequent_duplicates: Vec::new(),
        };
    }

    let main_files = selected_file_records(files, contexts, &scopes.main);
    let total_cleaned_lines = main_files
        .iter()
        .filter_map(|file| contexts.get(&file.relative_path))
        .map(cleaned_lines_for_duplication)
        .map(|lines| lines.len())
        .sum();
    let duplication_per_component = logical_decompositions
        .iter()
        .map(|logical_decomposition| {
            logical_decomposition
                .components
                .iter()
                .map(|component| {
                    let cleaned_lines = main_files
                        .iter()
                        .filter(|file| {
                            file_has_component(file, &logical_decomposition.key, &component.name)
                        })
                        .filter_map(|file| contexts.get(&file.relative_path))
                        .map(cleaned_lines_for_duplication)
                        .map(|lines| lines.len())
                        .sum();
                    duplication_metric(&component.name, 0, cleaned_lines, 0, 0)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    DuplicationAnalysisResultsExport {
        overall_duplication: duplication_metric("system", 0, total_cleaned_lines, 0, 0),
        duplication_per_component,
        duplication_per_concern: Vec::new(),
        duplication_per_extension: Vec::new(),
        longest_duplicates: Vec::new(),
        most_frequent_duplicates: Vec::new(),
    }
}

fn duplication_metric(
    key: &str,
    number_of_duplicates: usize,
    cleaned_lines_of_code: usize,
    duplicated_lines_of_code: usize,
    number_of_files_with_duplicates: usize,
) -> DuplicationMetricExport {
    DuplicationMetricExport {
        key: key.to_string(),
        number_of_duplicates,
        cleaned_lines_of_code,
        duplicated_lines_of_code,
        number_of_files_with_duplicates,
        duplication_percentage: if cleaned_lines_of_code == 0 {
            0.0
        } else {
            100.0 * duplicated_lines_of_code as f64 / cleaned_lines_of_code as f64
        },
    }
}

fn cleaned_lines_for_duplication(context: &ExportFileContext) -> Vec<String> {
    match context.file.extension.as_str() {
        "java" => cleaned_java_lines_for_duplication(&context.lines.join("\n")),
        _ => context
            .lines
            .iter()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect(),
    }
}

fn cleaned_java_lines_for_duplication(source: &str) -> Vec<String> {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let without_comments = strip_java_comments_keep_lines(&normalized);
    let mut cleaned = Vec::new();

    for line in split_lines(&without_comments) {
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if matches_entire("import .*;", &trimmed).unwrap_or(false)
            || matches_entire("package .*", &trimmed).unwrap_or(false)
            || matches_entire("[{]", &trimmed).unwrap_or(false)
            || matches_entire("[}]", &trimmed).unwrap_or(false)
        {
            continue;
        }
        cleaned.push(trimmed);
    }

    cleaned
}

fn strip_java_comments_keep_lines(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let chars = source.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut in_string: Option<char> = None;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if in_line_comment {
            if current == '\n' {
                in_line_comment = false;
                output.push('\n');
            }
            index += 1;
            continue;
        }

        if in_block_comment {
            if current == '*' && next == Some('/') {
                in_block_comment = false;
                index += 2;
                continue;
            }
            if current == '\n' {
                output.push('\n');
            }
            index += 1;
            continue;
        }

        if let Some(quote) = in_string {
            output.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == quote {
                in_string = None;
            }
            index += 1;
            continue;
        }

        if current == '/' && next == Some('/') {
            in_line_comment = true;
            index += 2;
            continue;
        }

        if current == '/' && next == Some('*') {
            in_block_comment = true;
            index += 2;
            continue;
        }

        if current == '"' || current == '\'' {
            in_string = Some(current);
        }
        output.push(current);
        index += 1;
    }

    output
}

fn build_metrics_list(
    files: &[FileRecord],
    main_aspect: &AnalysisAspectExport,
    test_aspect: &AnalysisAspectExport,
    generated_aspect: &AnalysisAspectExport,
    build_and_deploy_aspect: &AnalysisAspectExport,
    other_aspect: &AnalysisAspectExport,
    concerns_analysis: &[ConcernsGroupExport],
    logical_decompositions: &[LogicalDecompositionExport],
    files_analysis: &FilesAnalysisResultsExport,
    units_analysis: &UnitsAnalysisResultsExport,
    duplication_analysis: &DuplicationAnalysisResultsExport,
    main_dependencies: &[Dependency],
    total_analysis_time_ms: u64,
) -> Vec<MetricEntry> {
    let mut metrics = Vec::new();

    metrics.push(metric_entry(
        "TOTAL_NUMBER_OF_FILES",
        json!(files.len()),
        Some("Total number of files in the source folder"),
    ));

    append_aspect_metrics(&mut metrics, "MAIN", main_aspect);
    append_aspect_metrics(&mut metrics, "TEST", test_aspect);
    metrics.push(metric_entry(
        "TEST_VS_MAIN_LINES_OF_CODE_PERCENTAGE",
        json!(test_vs_main_percentage(
            test_aspect.lines_of_code,
            main_aspect.lines_of_code
        )),
        Some("Test / main code ratio"),
    ));
    append_aspect_metrics(&mut metrics, "GENERATED", generated_aspect);
    append_aspect_metrics(
        &mut metrics,
        "BUILD_AND_DEPLOYMENT",
        build_and_deploy_aspect,
    );
    append_aspect_metrics(&mut metrics, "OTHER", other_aspect);

    for logical_decomposition in logical_decompositions {
        for component in &logical_decomposition.components {
            let prefix = format!(
                "DECOMPOSITION_{}_{}",
                logical_decomposition.key, component.name
            );
            append_component_metrics(&mut metrics, &prefix, component);
        }

        let uses_built_in_dependency_finders = logical_decomposition
            .logical_decomposition
            .dependencies_finder
            .get("useBuiltInDependencyFinders")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        metrics.push(metric_entry(
            &format!(
                "NUMBER_OF_DEPENDENCY_LINKS_DECOMPOSITION_{}",
                logical_decomposition.key
            ),
            json!(if uses_built_in_dependency_finders {
                main_dependencies.len()
            } else {
                0
            }),
            Some("Number of anchor dependencies"),
        ));
        metrics.push(metric_entry(
            &format!(
                "NUMBER_OF_DEPENDENCIES_DECOMPOSITION_{}",
                logical_decomposition.key
            ),
            json!(logical_decomposition.component_dependencies.len()),
            None,
        ));
        metrics.push(metric_entry(
            &format!(
                "NUMBER_OF_PLACES_WITH_CYCLIC_DEPENDENCIES_DECOMPOSITION_{}",
                logical_decomposition.key
            ),
            json!(cyclic_component_dependency_places(
                &logical_decomposition.component_dependencies
            )),
            None,
        ));
    }

    for concerns_group in concerns_analysis {
        for concern in &concerns_group.concerns {
            let prefix = format!(
                "CONCERN_{}_{}",
                concerns_group.key,
                concern.name.clone().unwrap_or_default()
            );
            append_named_aspect_metrics(&mut metrics, &prefix, concern);
        }
    }

    append_file_size_metrics(&mut metrics, &files_analysis.overall_file_size_distribution);

    metrics.push(metric_entry(
        "NUMBER_OF_UNITS",
        json!(units_analysis.total_number_of_units),
        Some("Number of units"),
    ));
    metrics.push(metric_entry(
        "LINES_OF_CODE_IN_UNITS",
        json!(units_analysis.lines_of_code_in_units),
        Some("Lines of code in units"),
    ));
    metrics.push(metric_entry(
        "LINES_OF_CODE_OUTSIDE_UNITS",
        json!(
            main_aspect
                .lines_of_code
                .saturating_sub(units_analysis.lines_of_code_in_units)
        ),
        Some("Lines of code in units"),
    ));
    append_risk_metrics(
        &mut metrics,
        "Unit size ",
        &units_analysis.unit_size_risk_distribution,
    );
    for group in &units_analysis.unit_size_risk_distribution_per_component {
        for component in group {
            append_risk_metrics(
                &mut metrics,
                &format!("Unit Size Component {}: ", component.key),
                component,
            );
        }
    }
    for extension in &units_analysis.unit_size_risk_distribution_per_extension {
        append_risk_metrics(
            &mut metrics,
            &format!("Unit Size Extension {}: ", extension.key),
            extension,
        );
    }
    append_risk_metrics(
        &mut metrics,
        "Conditional complexity: ",
        &units_analysis.conditional_complexity_risk_distribution,
    );
    metrics.push(metric_entry(
        "CONDITIONAL_COMPLEXITY_HIGH_PLUS_RISK_COUNT",
        json!(
            units_analysis
                .conditional_complexity_risk_distribution
                .high_risk_count
                + units_analysis
                    .conditional_complexity_risk_distribution
                    .very_high_risk_count
        ),
        None,
    ));
    metrics.push(metric_entry(
        "CONDITIONAL_COMPLEXITY_HIGH_PLUS_RISK_LOC",
        json!(
            units_analysis
                .conditional_complexity_risk_distribution
                .high_risk_value
                + units_analysis
                    .conditional_complexity_risk_distribution
                    .very_high_risk_value
        ),
        None,
    ));
    for group in &units_analysis.conditional_complexity_risk_distribution_per_component {
        for component in group {
            append_risk_metrics(
                &mut metrics,
                &format!("Conditional Complexity Component {} ", component.key),
                component,
            );
        }
    }
    for extension in &units_analysis.conditional_complexity_risk_distribution_per_extension {
        append_risk_metrics(
            &mut metrics,
            &format!("Conditional Complexity Component {}: ", extension.key),
            extension,
        );
    }

    append_duplication_metrics(&mut metrics, duplication_analysis);
    metrics.push(metric_entry(
        "TOTAL_ANALYSIS_TIME_IN_MILLIS",
        json!(total_analysis_time_ms),
        Some("Total analysis time in milliseconds"),
    ));

    metrics
}

fn append_aspect_metrics(
    metrics: &mut Vec<MetricEntry>,
    aspect_name: &str,
    aspect: &AnalysisAspectExport,
) {
    append_named_aspect_metrics(metrics, aspect_name, aspect);
}

fn append_named_aspect_metrics(
    metrics: &mut Vec<MetricEntry>,
    name: &str,
    aspect: &AnalysisAspectExport,
) {
    metrics.push(metric_entry(
        &format!("NUMBER_OF_FILES_{name}"),
        json!(aspect.files_count),
        None,
    ));
    metrics.push(metric_entry(
        &format!("LINES_OF_CODE_{name}"),
        json!(aspect.lines_of_code),
        None,
    ));

    for metric in &aspect.file_count_per_extension {
        metrics.push(metric_entry(
            &format!(
                "NUMBER_OF_FILES_{}_EXT_{}",
                name,
                extension_metric_suffix(&metric.name)
            ),
            json!(metric.value),
            None,
        ));
        let extension_loc = aspect
            .lines_of_code_per_extension
            .iter()
            .find(|candidate| candidate.name == metric.name)
            .map(|candidate| candidate.value)
            .unwrap_or_default();
        metrics.push(metric_entry(
            &format!(
                "LINES_OF_CODE_{}_EXT_{}",
                name,
                extension_metric_suffix(&metric.name)
            ),
            json!(extension_loc),
            None,
        ));
    }
}

fn append_component_metrics(
    metrics: &mut Vec<MetricEntry>,
    name: &str,
    aspect: &AspectAnalysisExport,
) {
    metrics.push(metric_entry(
        &format!("NUMBER_OF_FILES_{name}"),
        json!(aspect.files_count),
        None,
    ));
    metrics.push(metric_entry(
        &format!("LINES_OF_CODE_{name}"),
        json!(aspect.lines_of_code),
        None,
    ));

    for metric in &aspect.file_count_per_extension {
        metrics.push(metric_entry(
            &format!(
                "NUMBER_OF_FILES_{}_EXT_{}",
                name,
                extension_metric_suffix(&metric.name)
            ),
            json!(metric.value),
            None,
        ));
        let extension_loc = aspect
            .lines_of_code_per_extension
            .iter()
            .find(|candidate| candidate.name == metric.name)
            .map(|candidate| candidate.value)
            .unwrap_or_default();
        metrics.push(metric_entry(
            &format!(
                "LINES_OF_CODE_{}_EXT_{}",
                name,
                extension_metric_suffix(&metric.name)
            ),
            json!(extension_loc),
            None,
        ));
    }
}

fn append_file_size_metrics(metrics: &mut Vec<MetricEntry>, distribution: &RiskDistributionExport) {
    let negligible_description = format!(
        " files with {} or less lines of code",
        distribution.low_risk_threshold
    );
    let low_description = format!(
        " files with {} to {} lines of code",
        distribution.low_risk_threshold, distribution.medium_risk_threshold
    );
    let medium_description = format!(
        " files with {} to {} lines of code",
        distribution.medium_risk_threshold, distribution.high_risk_threshold
    );
    let high_description = format!(
        " files with {} to {} lines of code",
        distribution.high_risk_threshold, distribution.very_high_risk_threshold
    );
    let very_high_description = format!(
        " files with more than {} lines of code",
        distribution.very_high_risk_threshold
    );

    metrics.push(metric_entry(
        "NEGLIGIBLE_RISK_FILE_SIZE_COUNT",
        json!(distribution.negligible_risk_count),
        Some(&format!("Number of {negligible_description}")),
    ));
    metrics.push(metric_entry(
        "LOW_RISK_FILE_SIZE_COUNT",
        json!(distribution.low_risk_count),
        Some(&format!("Number of {low_description}")),
    ));
    metrics.push(metric_entry(
        "MEDIUM_RISK_FILE_SIZE_COUNT",
        json!(distribution.medium_risk_count),
        Some(&format!("Number of {medium_description}")),
    ));
    metrics.push(metric_entry(
        "HIGH_RISK_FILE_SIZE_COUNT",
        json!(distribution.high_risk_count),
        Some(&format!("Number of {high_description}")),
    ));
    metrics.push(metric_entry(
        "VERY_HIGH_RISK_FILE_SIZE_COUNT",
        json!(distribution.very_high_risk_count),
        Some(&format!("Number of {very_high_description}")),
    ));

    metrics.push(metric_entry(
        "NEGLIGIBLE_RISK_FILE_SIZE_LOC",
        json!(distribution.negligible_risk_value),
        Some(&format!("Lines of code in {negligible_description}")),
    ));
    metrics.push(metric_entry(
        "LOW_RISK_FILE_SIZE_LOC",
        json!(distribution.low_risk_value),
        Some(&format!("Lines of code in {low_description}")),
    ));
    metrics.push(metric_entry(
        "MEDIUM_RISK_FILE_SIZE_LOC",
        json!(distribution.medium_risk_value),
        Some(&format!("Lines of code in {medium_description}")),
    ));
    metrics.push(metric_entry(
        "HIGH_RISK_FILE_SIZE_LOC",
        json!(distribution.high_risk_value),
        Some(&format!("Lines of code in {high_description}")),
    ));
    metrics.push(metric_entry(
        "VERY_HIGH_RISK_FILE_SIZE_LOC",
        json!(distribution.very_high_risk_value),
        Some(&format!("Lines of code in {very_high_description}")),
    ));
}

fn append_risk_metrics(
    metrics: &mut Vec<MetricEntry>,
    display_prefix: &str,
    distribution: &RiskDistributionExport,
) {
    let name_prefix = safe_metric_prefix(display_prefix);
    append_single_risk_metrics(
        metrics,
        &name_prefix,
        "NEGLIGIBLE",
        distribution.negligible_risk_value,
        distribution.negligible_risk_percentage,
        distribution.negligible_risk_count,
    );
    append_single_risk_metrics(
        metrics,
        &name_prefix,
        "LOW",
        distribution.low_risk_value,
        distribution.low_risk_percentage,
        distribution.low_risk_count,
    );
    append_single_risk_metrics(
        metrics,
        &name_prefix,
        "MEDIUM",
        distribution.medium_risk_value,
        distribution.medium_risk_percentage,
        distribution.medium_risk_count,
    );
    append_single_risk_metrics(
        metrics,
        &name_prefix,
        "HIGH",
        distribution.high_risk_value,
        distribution.high_risk_percentage,
        distribution.high_risk_count,
    );
    append_single_risk_metrics(
        metrics,
        &name_prefix,
        "VERY_HIGH",
        distribution.very_high_risk_value,
        distribution.very_high_risk_percentage,
        distribution.very_high_risk_count,
    );
}

fn append_single_risk_metrics(
    metrics: &mut Vec<MetricEntry>,
    prefix: &str,
    category: &str,
    loc_value: usize,
    percentage: f64,
    count: usize,
) {
    metrics.push(metric_entry(
        &format!("{prefix}_{category}_RISK_LOC"),
        json!(loc_value),
        None,
    ));
    metrics.push(metric_entry(
        &format!("{prefix}_{category}_RISK_PERCENTAGE"),
        json!(percentage),
        None,
    ));
    metrics.push(metric_entry(
        &format!("{prefix}_{category}_RISK_COUNT"),
        json!(count),
        None,
    ));
}

fn append_duplication_metrics(
    metrics: &mut Vec<MetricEntry>,
    duplication_analysis: &DuplicationAnalysisResultsExport,
) {
    metrics.push(metric_entry(
        "DUPLICATION_NUMBER_OF_DUPLICATES",
        json!(
            duplication_analysis
                .overall_duplication
                .number_of_duplicates
        ),
        Some("Number of duplicates"),
    ));
    metrics.push(metric_entry(
        "DUPLICATION_NUMBER_OF_FILES_WITH_DUPLICATES",
        json!(
            duplication_analysis
                .overall_duplication
                .number_of_files_with_duplicates
        ),
        Some("Number of files with duplicates"),
    ));
    metrics.push(metric_entry(
        "DUPLICATION_NUMBER_OF_DUPLICATED_LINES",
        json!(
            duplication_analysis
                .overall_duplication
                .duplicated_lines_of_code
        ),
        Some("Number of duplicated lines"),
    ));
    metrics.push(metric_entry(
        "DUPLICATION_NUMBER_OF_CLEANED_LINES",
        json!(
            duplication_analysis
                .overall_duplication
                .cleaned_lines_of_code
        ),
        Some("Number of lines after cleaning for duplication calculations"),
    ));
    metrics.push(metric_entry(
        "DUPLICATION_PERCENTAGE",
        division_value(
            duplication_analysis
                .overall_duplication
                .duplicated_lines_of_code,
            duplication_analysis
                .overall_duplication
                .cleaned_lines_of_code,
        ),
        Some("Duplication percentage"),
    ));
    metrics.push(metric_entry(
        "UNIT_DUPLICATES_COUNT",
        json!(0),
        Some("Unit duplicates"),
    ));

    for group in &duplication_analysis.duplication_per_component {
        for component in group {
            metrics.push(metric_entry(
                &format!(
                    "DUPLICATION_NUMBER_OF_DUPLICATED_LINES_PRIMARY_{}",
                    component.key
                ),
                json!(component.duplicated_lines_of_code),
                Some("Number of duplicated lines"),
            ));
            metrics.push(metric_entry(
                &format!(
                    "DUPLICATION_NUMBER_OF_CLEANED_LINES_PRIMARY_{}",
                    component.key
                ),
                json!(component.duplicated_lines_of_code),
                Some("Number of lines after cleaning for duplication calculations"),
            ));
            metrics.push(metric_entry(
                &format!("DUPLICATION_PERCENTAGE_PRIMARY_{}", component.key),
                division_value(
                    component.duplicated_lines_of_code,
                    component.duplicated_lines_of_code,
                ),
                Some("Duplication percentage"),
            ));
        }
    }
}

fn metric_entry(id: &str, value: Value, description: Option<&str>) -> MetricEntry {
    MetricEntry {
        id: metric_id(id),
        value,
        description: description.map(str::to_owned),
    }
}

fn extension_metric_suffix(name: &str) -> String {
    name.replace("*.", "").trim().to_ascii_uppercase()
}

fn safe_metric_prefix(prefix: &str) -> String {
    safe_file_name(&prefix.to_ascii_uppercase().replace(':', ""))
}

fn metric_id(name: &str) -> String {
    safe_file_name(&analysis_metric_name(name))
}

fn analysis_metric_name(name: &str) -> String {
    let mut value = String::new();
    let mut in_parentheses = false;
    for ch in name.chars() {
        match ch {
            '(' => in_parentheses = true,
            ')' => in_parentheses = false,
            _ if !in_parentheses => value.push(ch),
            _ => {}
        }
    }

    let mut value = value.replace(' ', "_").replace('-', "_");
    while value.contains("__") {
        value = value.replace("__", "_");
    }
    value.trim_end_matches('_').trim().to_ascii_uppercase()
}

fn safe_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch == '.' || ch.is_ascii_alphanumeric() {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .to_ascii_uppercase()
}

fn build_controls(goals: &[MetricsWithGoalConfig], metrics: &[MetricEntry]) -> Value {
    let metrics_map = metrics
        .iter()
        .map(|metric| (metric.id.to_ascii_uppercase(), metric.clone()))
        .collect::<BTreeMap<_, _>>();

    json!({
        "goalsAnalysisResults": goals.iter().map(|goal| {
            let control_statuses = goal.controls.iter().map(|control| {
                if let Some(metric) = metrics_map.get(&control.metric.to_ascii_uppercase()) {
                    json!({
                        "control": control,
                        "metric": metric,
                        "status": control_status(control, &metric.value),
                    })
                } else {
                    json!({
                        "control": control,
                        "metric": {
                            "id": metric_id(&control.metric),
                            "value": Value::Null,
                            "description": Value::Null,
                        },
                        "status": "IGNORED: the metric not found",
                    })
                }
            }).collect::<Vec<_>>();
            json!({
                "metricsWithGoal": goal,
                "controlStatuses": control_statuses,
            })
        }).collect::<Vec<_>>()
    })
}

fn control_status(control: &crate::MetricRangeControlConfig, value: &Value) -> &'static str {
    let Some(value) = metric_value_as_f64(value) else {
        return "FAILED";
    };

    if is_in_range(&control.desired_range, value, 0.0) {
        "OK"
    } else if is_in_range(
        &control.desired_range,
        value,
        range_tolerance(&control.desired_range),
    ) {
        "TOLERANT"
    } else {
        "FAILED"
    }
}

fn metric_value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) if text.eq_ignore_ascii_case("NaN") => Some(f64::NAN),
        Value::String(text) => text.parse::<f64>().ok(),
        _ => None,
    }
}

fn is_in_range(range: &RangeConfig, value: f64, tolerance: f64) -> bool {
    if value.is_nan() {
        return false;
    }

    is_bigger_than(&range.min, value, tolerance) && is_smaller_than(&range.max, value, tolerance)
}

fn is_bigger_than(test_value: &str, value: f64, tolerance: f64) -> bool {
    if test_value.trim().is_empty() {
        return true;
    }

    test_value
        .trim()
        .parse::<f64>()
        .ok()
        .is_some_and(|threshold| value >= threshold - tolerance)
}

fn is_smaller_than(test_value: &str, value: f64, tolerance: f64) -> bool {
    if test_value.trim().is_empty() {
        return true;
    }

    test_value
        .trim()
        .parse::<f64>()
        .ok()
        .is_some_and(|threshold| value <= threshold + tolerance)
}

fn range_tolerance(range: &RangeConfig) -> f64 {
    range.tolerance.trim().parse::<f64>().unwrap_or_default()
}

fn test_vs_main_percentage(test_lines: usize, main_lines: usize) -> f64 {
    if main_lines == 0 {
        0.0
    } else {
        (100.0 * test_lines as f64 / main_lines as f64 * 100.0).round() / 100.0
    }
}

fn cyclic_component_dependency_places(
    component_dependencies: &[ComponentDependencyExport],
) -> usize {
    let mut count = 0;

    for (index, dependency) in component_dependencies.iter().enumerate() {
        for (other_index, other_dependency) in component_dependencies.iter().enumerate() {
            if index == other_index {
                continue;
            }
            if dependency.from_component == other_dependency.to_component
                && dependency.to_component == other_dependency.from_component
            {
                count += 1;
            }
        }
    }

    count / 2
}

fn division_value(numerator: usize, denominator: usize) -> Value {
    if denominator == 0 {
        Value::String(String::from("NaN"))
    } else {
        json!(100.0 * numerator as f64 / denominator as f64)
    }
}

fn build_file_size_distributions_per_extension(
    files: &[FileRecord],
    thresholds: &ThresholdsConfig,
) -> Vec<RiskDistributionExport> {
    let mut extensions = Vec::<String>::new();
    for file in files {
        if !extensions.contains(&file.extension) {
            extensions.push(file.extension.clone());
        }
    }

    extensions
        .into_iter()
        .map(|extension| {
            build_risk_distribution(
                &extension,
                thresholds,
                files
                    .iter()
                    .filter(|file| file.extension == extension)
                    .map(|file| (file.lines_of_code, file.lines_of_code)),
            )
        })
        .collect()
}

fn build_risk_distribution(
    key: &str,
    thresholds: &ThresholdsConfig,
    values: impl IntoIterator<Item = (usize, usize)>,
) -> RiskDistributionExport {
    let low = threshold_low(thresholds);
    let medium = threshold_medium(thresholds);
    let high = threshold_high(thresholds);
    let very_high = threshold_very_high(thresholds);
    let mut distribution = RiskDistributionExport {
        key: key.to_string(),
        low_risk_threshold: low,
        medium_risk_threshold: medium,
        high_risk_threshold: high,
        very_high_risk_threshold: very_high,
        negligible_risk_value: 0,
        low_risk_value: 0,
        medium_risk_value: 0,
        high_risk_value: 0,
        very_high_risk_value: 0,
        low_risk_count: 0,
        negligible_risk_count: 0,
        medium_risk_count: 0,
        high_risk_count: 0,
        very_high_risk_count: 0,
        negligible_risk_label: format!("1-{low}"),
        low_risk_label: format!("{}-{medium}", low + 1),
        medium_risk_label: format!("{}-{high}", medium + 1),
        high_risk_label: format!("{}-{very_high}", high + 1),
        very_high_risk_label: format!("{}+", very_high + 1),
        value_unit: String::from("LOC"),
        count_unit: String::from("files"),
        negligible_risk_percentage: 0.0,
        total_value: 0,
        total_count: 0,
        very_high_risk_percentage: 0.0,
        high_risk_percentage: 0.0,
        medium_risk_percentage: 0.0,
        low_risk_percentage: 0.0,
    };

    for (test_value, add_value) in values {
        if test_value <= low {
            distribution.negligible_risk_value += add_value;
            distribution.negligible_risk_count += 1;
        } else if test_value <= medium {
            distribution.low_risk_value += add_value;
            distribution.low_risk_count += 1;
        } else if test_value <= high {
            distribution.medium_risk_value += add_value;
            distribution.medium_risk_count += 1;
        } else if test_value <= very_high {
            distribution.high_risk_value += add_value;
            distribution.high_risk_count += 1;
        } else {
            distribution.very_high_risk_value += add_value;
            distribution.very_high_risk_count += 1;
        }
    }

    distribution.total_value = distribution.negligible_risk_value
        + distribution.low_risk_value
        + distribution.medium_risk_value
        + distribution.high_risk_value
        + distribution.very_high_risk_value;
    distribution.total_count = distribution.negligible_risk_count
        + distribution.low_risk_count
        + distribution.medium_risk_count
        + distribution.high_risk_count
        + distribution.very_high_risk_count;
    if distribution.total_value > 0 {
        distribution.negligible_risk_percentage =
            100.0 * distribution.negligible_risk_value as f64 / distribution.total_value as f64;
        distribution.low_risk_percentage =
            100.0 * distribution.low_risk_value as f64 / distribution.total_value as f64;
        distribution.medium_risk_percentage =
            100.0 * distribution.medium_risk_value as f64 / distribution.total_value as f64;
        distribution.high_risk_percentage =
            100.0 * distribution.high_risk_value as f64 / distribution.total_value as f64;
        distribution.very_high_risk_percentage =
            100.0 * distribution.very_high_risk_value as f64 / distribution.total_value as f64;
    }

    distribution
}

fn threshold_low(thresholds: &ThresholdsConfig) -> usize {
    thresholds.low.max(1)
}

fn threshold_medium(thresholds: &ThresholdsConfig) -> usize {
    thresholds.medium.max(threshold_low(thresholds) + 1)
}

fn threshold_high(thresholds: &ThresholdsConfig) -> usize {
    thresholds.high.max(threshold_medium(thresholds) + 1)
}

fn threshold_very_high(thresholds: &ThresholdsConfig) -> usize {
    thresholds.very_high.max(threshold_high(thresholds) + 1)
}

fn filter_units(units: &[Unit], selected_paths: &BTreeSet<String>) -> Vec<Unit> {
    units
        .iter()
        .filter(|unit| selected_paths.contains(&unit.relative_file_name))
        .cloned()
        .collect()
}

fn filter_dependencies(
    dependencies: &[Dependency],
    selected_paths: &BTreeSet<String>,
) -> Vec<Dependency> {
    dependencies
        .iter()
        .filter_map(|dependency| {
            let mut filtered = dependency.clone();
            filtered
                .from_files
                .retain(|from_file| selected_paths.contains(&from_file.file.relative_path));
            filtered
                .from_anchor
                .files
                .retain(|file| selected_paths.contains(&file.relative_path));
            filtered
                .to_anchor
                .files
                .retain(|file| selected_paths.contains(&file.relative_path));

            if filtered.from_files.is_empty()
                && filtered.from_anchor.files.is_empty()
                && filtered.to_anchor.files.is_empty()
            {
                None
            } else {
                Some(filtered)
            }
        })
        .collect()
}

fn file_has_component(file: &FileRecord, decomposition: &str, component: &str) -> bool {
    file.components
        .iter()
        .any(|candidate| candidate == &format!("{decomposition}::{component}"))
}

fn unit_has_component(unit: &Unit, decomposition: &str, component: &str) -> bool {
    unit.components
        .iter()
        .any(|candidate| candidate == &format!("{decomposition}::{component}"))
}

fn current_unix_millis() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("compute current unix time")?
        .as_millis() as u64)
}

fn current_local_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
