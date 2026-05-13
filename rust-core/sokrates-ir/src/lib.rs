use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryAnalysis {
    pub metadata: Metadata,
    pub summary: AnalysisSummary,
    #[serde(default)]
    pub files: Vec<FileRecord>,
    #[serde(default)]
    pub units: Vec<Unit>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub component_dependencies: Vec<ComponentDependency>,
    #[serde(default)]
    pub diagnostics: Vec<ParseDiagnostic>,
}

impl RepositoryAnalysis {
    pub fn with_files(metadata: Metadata, files: Vec<FileRecord>) -> Self {
        Self {
            summary: AnalysisSummary {
                total_files: files.len(),
            },
            metadata,
            files,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSummary {
    pub total_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub relative_path: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Unit {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub short_name: String,
    #[serde(default)]
    pub long_name: String,
    #[serde(default)]
    pub relative_file_name: String,
    #[serde(default)]
    pub file_lines_count: usize,
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub start_line: usize,
    #[serde(default)]
    pub end_line: usize,
    #[serde(default)]
    pub lines_of_code: usize,
    #[serde(default)]
    pub mc_cabe_index: usize,
    #[serde(default)]
    pub number_of_parameters: usize,
    #[serde(default)]
    pub number_of_literals: usize,
    #[serde(default)]
    pub number_of_statements: usize,
    #[serde(default)]
    pub number_of_expressions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub from_anchor: DependencyAnchor,
    #[serde(default)]
    pub to_anchor: DependencyAnchor,
    #[serde(default)]
    pub from_files: Vec<SourceFileDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DependencyAnchor {
    #[serde(default)]
    pub anchor: String,
    #[serde(default)]
    pub code_fragment: String,
    #[serde(default)]
    pub dependency_patterns: Vec<String>,
    #[serde(default)]
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceFileDependency {
    #[serde(default)]
    pub file: FileRecord,
    #[serde(default)]
    pub code_fragment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDependency {
    #[serde(default)]
    pub decomposition: String,
    #[serde(default)]
    pub from_component: String,
    #[serde(default)]
    pub loc_from: usize,
    #[serde(default)]
    pub evidence: Vec<DependencyEvidence>,
    #[serde(default)]
    pub to_component: String,
    #[serde(default)]
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DependencyEvidence {
    #[serde(default)]
    pub path_from: String,
    #[serde(default)]
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParseDiagnostic {
    pub message: String,
    pub relative_path: String,
    pub severity: DiagnosticSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Info,
    #[default]
    Warning,
    Error,
}

#[cfg(test)]
mod tests {
    use super::{FileRecord, Metadata, RepositoryAnalysis};

    #[test]
    fn repository_analysis_round_trips_json() {
        let analysis = RepositoryAnalysis::with_files(
            Metadata {
                name: String::from("sample"),
                description: String::from("fixture"),
            },
            vec![FileRecord {
                relative_path: String::from("src/main/java/demo/HelloService.java"),
                extension: String::from("java"),
                lines_of_code: 0,
                components: Vec::new(),
                concerns: Vec::new(),
                units_count: 0,
                units_mc_cabe_index_sum: 0,
                lines_of_code_in_units: 0,
            }],
        );

        let json = serde_json::to_string_pretty(&analysis).expect("serialize analysis");
        let restored: RepositoryAnalysis =
            serde_json::from_str(&json).expect("deserialize analysis");

        assert_eq!(restored, analysis);
    }
}
