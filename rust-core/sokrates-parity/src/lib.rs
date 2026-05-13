use anyhow::{Context, Result};
use serde::Deserialize;
use sokrates_ir::{Metadata, Unit};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JavaGoldenAnalysisResults {
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub main_aspect_analysis_results: JavaAspectAnalysisResults,
    #[serde(default)]
    pub test_aspect_analysis_results: JavaAspectAnalysisResults,
    #[serde(default)]
    pub generated_aspect_analysis_results: JavaAspectAnalysisResults,
    #[serde(default)]
    pub build_and_deploy_aspect_analysis_results: JavaAspectAnalysisResults,
    #[serde(default)]
    pub other_aspect_analysis_results: JavaAspectAnalysisResults,
}

impl JavaGoldenAnalysisResults {
    pub fn display_name(&self) -> &str {
        &self.metadata.name
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaAspectAnalysisResults {
    #[serde(default)]
    pub aspect: JavaAspect,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaAspect {
    #[serde(default)]
    pub source_files: Vec<JavaSourceFile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct JavaSourceFile {
    pub relative_path: String,
}

pub fn load_java_golden(path: &Path) -> Result<JavaGoldenAnalysisResults> {
    let content =
        fs::read_to_string(path).with_context(|| format!("read golden {}", path.display()))?;

    serde_json::from_str(&content).with_context(|| format!("parse golden {}", path.display()))
}

pub fn load_java_golden_inventory(root: &Path) -> Result<Vec<String>> {
    let mut paths = BTreeSet::new();

    for file_name in [
        "mainFiles.json",
        "testFiles.json",
        "generatedFiles.json",
        "buildAndDeploymentFiles.json",
        "otherFiles.json",
    ] {
        let file_path = root.join(file_name);
        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("read golden file inventory {}", file_path.display()))?;
        let entries: Vec<JavaFileListEntry> = serde_json::from_str(&content)
            .with_context(|| format!("parse golden file inventory {}", file_path.display()))?;

        for entry in entries {
            paths.insert(entry.relative_path);
        }
    }

    Ok(paths.into_iter().collect())
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct JavaFileListEntry {
    relative_path: String,
}

pub fn load_java_units_golden(path: &Path) -> Result<Vec<Unit>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("read units golden {}", path.display()))?;

    serde_json::from_str(&content).with_context(|| format!("parse units golden {}", path.display()))
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaGoldenDependency {
    #[serde(rename = "from", default)]
    pub from_anchor: JavaGoldenDependencyAnchor,
    #[serde(rename = "to", default)]
    pub to_anchor: JavaGoldenDependencyAnchor,
    #[serde(default)]
    pub from_files: Vec<JavaGoldenSourceFileDependency>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaGoldenDependencyAnchor {
    #[serde(default)]
    pub anchor: String,
    #[serde(default)]
    pub code_fragment: String,
    #[serde(default)]
    pub dependency_patterns: Vec<String>,
    #[serde(default)]
    pub files: Vec<JavaGoldenFileRecord>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaGoldenSourceFileDependency {
    #[serde(default)]
    pub file: JavaGoldenFileRecord,
    #[serde(default)]
    pub code_fragment: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaGoldenFileRecord {
    #[serde(default)]
    pub relative_path: String,
    #[serde(default)]
    pub extension: String,
    #[serde(default)]
    pub lines_of_code: usize,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub concerns: Vec<String>,
    #[serde(default)]
    pub units_count: usize,
    #[serde(default)]
    pub units_mc_cabe_index_sum: usize,
    #[serde(default)]
    pub lines_of_code_in_units: usize,
}

pub fn load_java_dependencies_golden(path: &Path) -> Result<Vec<JavaGoldenDependency>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("read dependencies golden {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| format!("parse dependencies golden {}", path.display()))
}

pub fn load_java_file_metrics_golden(path: &Path) -> Result<Vec<JavaGoldenFileRecord>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("read file metrics golden {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| format!("parse file metrics golden {}", path.display()))
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaGoldenComponentDependency {
    #[serde(skip)]
    pub decomposition: String,
    #[serde(default)]
    pub from_component: String,
    #[serde(default)]
    pub loc_from: usize,
    #[serde(default)]
    pub evidence: Vec<JavaGoldenDependencyEvidence>,
    #[serde(default)]
    pub to_component: String,
    #[serde(default)]
    pub count: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JavaGoldenDependencyEvidence {
    #[serde(default)]
    pub path_from: String,
    #[serde(default)]
    pub evidence: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
struct JavaGoldenLogicalDecomposition {
    #[serde(default)]
    key: String,
    #[serde(default)]
    component_dependencies: Vec<JavaGoldenComponentDependency>,
}

pub fn load_java_component_dependencies_golden(
    path: &Path,
) -> Result<Vec<JavaGoldenComponentDependency>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("read logical decompositions golden {}", path.display()))?;
    let decompositions = serde_json::from_str::<Vec<JavaGoldenLogicalDecomposition>>(&content)
        .with_context(|| format!("parse logical decompositions golden {}", path.display()))?;
    let mut dependencies = Vec::new();

    for logical_decomposition in decompositions {
        for mut dependency in logical_decomposition.component_dependencies {
            dependency.decomposition = logical_decomposition.key.clone();
            dependencies.push(dependency);
        }
    }

    dependencies.sort_by(|left, right| {
        (
            &left.decomposition,
            &left.from_component,
            &left.to_component,
        )
            .cmp(&(
                &right.decomposition,
                &right.from_component,
                &right.to_component,
            ))
    });

    Ok(dependencies)
}
