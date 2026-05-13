use anyhow::Result;
use sokrates_cli::analyze_from_cli_options;
use sokrates_ir::{
    ComponentDependency, Dependency, DependencyAnchor, DependencyEvidence, FileRecord,
    SourceFileDependency,
};
use sokrates_parity::{
    JavaGoldenComponentDependency, JavaGoldenDependency, JavaGoldenDependencyAnchor,
    JavaGoldenDependencyEvidence, JavaGoldenFileRecord, JavaGoldenSourceFileDependency,
    load_java_component_dependencies_golden, load_java_dependencies_golden,
};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ComparableFileRecord {
    relative_path: String,
    extension: String,
    lines_of_code: usize,
    components: Vec<String>,
}

impl From<&FileRecord> for ComparableFileRecord {
    fn from(file: &FileRecord) -> Self {
        Self {
            relative_path: file.relative_path.clone(),
            extension: file.extension.clone(),
            lines_of_code: file.lines_of_code,
            components: file.components.clone(),
        }
    }
}

impl From<&JavaGoldenFileRecord> for ComparableFileRecord {
    fn from(file: &JavaGoldenFileRecord) -> Self {
        Self {
            relative_path: file.relative_path.clone(),
            extension: file.extension.clone(),
            lines_of_code: file.lines_of_code,
            components: file.components.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparableDependencyAnchor {
    anchor: String,
    code_fragment: String,
    dependency_patterns: Vec<String>,
    files: Vec<ComparableFileRecord>,
}

impl From<&DependencyAnchor> for ComparableDependencyAnchor {
    fn from(anchor: &DependencyAnchor) -> Self {
        let mut files = anchor
            .files
            .iter()
            .map(ComparableFileRecord::from)
            .collect::<Vec<_>>();
        files.sort();

        Self {
            anchor: anchor.anchor.clone(),
            code_fragment: anchor.code_fragment.clone(),
            dependency_patterns: anchor.dependency_patterns.clone(),
            files,
        }
    }
}

impl From<&JavaGoldenDependencyAnchor> for ComparableDependencyAnchor {
    fn from(anchor: &JavaGoldenDependencyAnchor) -> Self {
        let mut files = anchor
            .files
            .iter()
            .map(ComparableFileRecord::from)
            .collect::<Vec<_>>();
        files.sort();

        Self {
            anchor: anchor.anchor.clone(),
            code_fragment: anchor.code_fragment.clone(),
            dependency_patterns: anchor.dependency_patterns.clone(),
            files,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ComparableSourceFileDependency {
    file: ComparableFileRecord,
    code_fragment: String,
}

impl From<&SourceFileDependency> for ComparableSourceFileDependency {
    fn from(source_file_dependency: &SourceFileDependency) -> Self {
        Self {
            file: ComparableFileRecord::from(&source_file_dependency.file),
            code_fragment: source_file_dependency.code_fragment.clone(),
        }
    }
}

impl From<&JavaGoldenSourceFileDependency> for ComparableSourceFileDependency {
    fn from(source_file_dependency: &JavaGoldenSourceFileDependency) -> Self {
        Self {
            file: ComparableFileRecord::from(&source_file_dependency.file),
            code_fragment: source_file_dependency.code_fragment.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparableDependency {
    from: String,
    to: String,
    kind: String,
    from_anchor: ComparableDependencyAnchor,
    to_anchor: ComparableDependencyAnchor,
    from_files: Vec<ComparableSourceFileDependency>,
}

impl From<&Dependency> for ComparableDependency {
    fn from(dependency: &Dependency) -> Self {
        let mut from_files = dependency
            .from_files
            .iter()
            .map(ComparableSourceFileDependency::from)
            .collect::<Vec<_>>();
        from_files.sort();

        Self {
            from: dependency.from.clone(),
            to: dependency.to.clone(),
            kind: dependency.kind.clone(),
            from_anchor: ComparableDependencyAnchor::from(&dependency.from_anchor),
            to_anchor: ComparableDependencyAnchor::from(&dependency.to_anchor),
            from_files,
        }
    }
}

impl From<&JavaGoldenDependency> for ComparableDependency {
    fn from(dependency: &JavaGoldenDependency) -> Self {
        let mut from_files = dependency
            .from_files
            .iter()
            .map(ComparableSourceFileDependency::from)
            .collect::<Vec<_>>();
        from_files.sort();

        Self {
            from: dependency.from_anchor.anchor.clone(),
            to: dependency.to_anchor.anchor.clone(),
            kind: String::from("package"),
            from_anchor: ComparableDependencyAnchor::from(&dependency.from_anchor),
            to_anchor: ComparableDependencyAnchor::from(&dependency.to_anchor),
            from_files,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ComparableDependencyEvidence {
    path_from: String,
    evidence: String,
}

impl From<&DependencyEvidence> for ComparableDependencyEvidence {
    fn from(evidence: &DependencyEvidence) -> Self {
        Self {
            path_from: evidence.path_from.clone(),
            evidence: evidence.evidence.clone(),
        }
    }
}

impl From<&JavaGoldenDependencyEvidence> for ComparableDependencyEvidence {
    fn from(evidence: &JavaGoldenDependencyEvidence) -> Self {
        Self {
            path_from: evidence.path_from.clone(),
            evidence: evidence.evidence.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComparableComponentDependency {
    decomposition: String,
    from_component: String,
    to_component: String,
    count: usize,
    loc_from: usize,
    evidence: Vec<ComparableDependencyEvidence>,
}

impl From<&ComponentDependency> for ComparableComponentDependency {
    fn from(dependency: &ComponentDependency) -> Self {
        let mut evidence = dependency
            .evidence
            .iter()
            .map(ComparableDependencyEvidence::from)
            .collect::<Vec<_>>();
        evidence.sort();

        Self {
            decomposition: dependency.decomposition.clone(),
            from_component: dependency.from_component.clone(),
            to_component: dependency.to_component.clone(),
            count: dependency.count,
            loc_from: dependency.loc_from,
            evidence,
        }
    }
}

impl From<&JavaGoldenComponentDependency> for ComparableComponentDependency {
    fn from(dependency: &JavaGoldenComponentDependency) -> Self {
        let mut evidence = dependency
            .evidence
            .iter()
            .map(ComparableDependencyEvidence::from)
            .collect::<Vec<_>>();
        evidence.sort();

        Self {
            decomposition: dependency.decomposition.clone(),
            from_component: dependency.from_component.clone(),
            to_component: dependency.to_component.clone(),
            count: dependency.count,
            loc_from: dependency.loc_from,
            evidence,
        }
    }
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("java-dependencies-sample")
}

#[test]
fn java_dependencies_sample_matches_java_dependency_goldens() -> Result<()> {
    let fixture = fixture_root();
    let config = fixture.join("input").join("_sokrates").join("config.json");
    let dependencies_golden = fixture.join("goldens").join("dependencies.json");
    let logical_decompositions_golden = fixture.join("goldens").join("logical_decompositions.json");

    let analysis = analyze_from_cli_options(None, Some(config))?;
    let expected_dependencies = load_java_dependencies_golden(&dependencies_golden)?;
    let expected_component_dependencies =
        load_java_component_dependencies_golden(&logical_decompositions_golden)?;

    let actual_dependencies = analysis
        .dependencies
        .iter()
        .map(ComparableDependency::from)
        .collect::<Vec<_>>();
    let expected_dependencies = expected_dependencies
        .iter()
        .map(ComparableDependency::from)
        .collect::<Vec<_>>();

    assert_eq!(actual_dependencies, expected_dependencies);

    let actual_component_dependencies = analysis
        .component_dependencies
        .iter()
        .map(ComparableComponentDependency::from)
        .collect::<Vec<_>>();
    let expected_component_dependencies = expected_component_dependencies
        .iter()
        .map(ComparableComponentDependency::from)
        .collect::<Vec<_>>();

    assert_eq!(
        actual_component_dependencies,
        expected_component_dependencies
    );

    Ok(())
}
