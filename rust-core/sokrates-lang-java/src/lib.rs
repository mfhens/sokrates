use anyhow::{Context, Result};
use regex::Regex;
use sokrates_ir::{
    ComponentDependency, Dependency, DependencyAnchor, DependencyEvidence, DiagnosticSeverity,
    FileRecord, ParseDiagnostic, SourceFileDependency, Unit,
};
use sokrates_ts_core::{SupportedLanguage, node_span, parse};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

const JAVA_PACKAGE_PREFIX: &str = "package ";
const MAX_REGEX_CONTENT_LENGTH: usize = 1000;

#[derive(Debug, Default)]
pub struct JavaFileAnalysis {
    pub units: Vec<Unit>,
    pub diagnostics: Vec<ParseDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct JavaSourceInput {
    pub relative_path: String,
    pub source: String,
    pub file: FileRecord,
}

#[derive(Debug, Default)]
pub struct JavaRepositoryAnalysis {
    pub files: Vec<FileRecord>,
    pub units: Vec<Unit>,
    pub dependencies: Vec<Dependency>,
    pub component_dependencies: Vec<ComponentDependency>,
    pub diagnostics: Vec<ParseDiagnostic>,
}

pub fn analyze_file(
    relative_path: &str,
    source: &str,
    components: Vec<String>,
) -> Result<JavaFileAnalysis> {
    let document = parse(SupportedLanguage::Java, source)?;
    let file_lines_count = source.lines().count();
    let mut diagnostics = Vec::new();

    if document.has_errors() {
        diagnostics.push(ParseDiagnostic {
            message: String::from("Tree-sitter reported syntax errors while parsing Java source."),
            relative_path: String::from(relative_path),
            severity: DiagnosticSeverity::Warning,
        });
    }

    let mut unit_nodes = Vec::new();
    collect_unit_nodes(document.tree().root_node(), &mut unit_nodes);

    let mut units = unit_nodes
        .into_iter()
        .map(|node| build_unit(relative_path, source, file_lines_count, &components, node))
        .collect::<Result<Vec<_>>>()?;
    units.sort_by(|left, right| right.lines_of_code.cmp(&left.lines_of_code));

    Ok(JavaFileAnalysis { units, diagnostics })
}

pub fn analyze_repository(inputs: &[JavaSourceInput]) -> Result<JavaRepositoryAnalysis> {
    let normalized_inputs = inputs
        .iter()
        .map(normalize_repository_input)
        .collect::<Vec<_>>();
    let mut units = Vec::new();
    let mut diagnostics = Vec::new();

    for input in &normalized_inputs {
        let analysis = analyze_file(
            &input.relative_path,
            &input.source,
            input.file.components.clone(),
        )?;
        units.extend(analysis.units);
        diagnostics.extend(analysis.diagnostics);
    }

    units.sort_by(|left, right| right.lines_of_code.cmp(&left.lines_of_code));
    let files = build_file_records(&normalized_inputs, &units);

    let anchors = collect_dependency_anchors(&normalized_inputs);
    let dependencies = extract_package_dependencies(&normalized_inputs, &anchors)?;
    let component_dependencies = extract_component_dependencies(&dependencies);

    Ok(JavaRepositoryAnalysis {
        files,
        units,
        dependencies,
        component_dependencies,
        diagnostics,
    })
}

fn normalize_repository_input(input: &JavaSourceInput) -> JavaSourceInput {
    let mut file = input.file.clone();
    file.lines_of_code = cleaned_lines_of_code(&input.source);

    JavaSourceInput {
        relative_path: input.relative_path.clone(),
        source: input.source.clone(),
        file,
    }
}

fn build_file_records(inputs: &[JavaSourceInput], units: &[Unit]) -> Vec<FileRecord> {
    let mut files = inputs
        .iter()
        .map(|input| (input.relative_path.clone(), input.file.clone()))
        .collect::<BTreeMap<_, _>>();

    for unit in units {
        let Some(file) = files.get_mut(&unit.relative_file_name) else {
            continue;
        };

        file.units_count += 1;
        file.units_mc_cabe_index_sum += unit.mc_cabe_index;
        file.lines_of_code_in_units += unit.lines_of_code;
    }

    files.into_values().collect()
}

fn collect_unit_nodes<'a>(node: Node<'a>, nodes: &mut Vec<Node<'a>>) {
    if is_unit_node(node) {
        nodes.push(node);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_unit_nodes(child, nodes);
    }
}

fn is_unit_node(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "method_declaration" | "constructor_declaration" | "static_initializer"
    )
}

fn build_unit(
    relative_path: &str,
    source: &str,
    file_lines_count: usize,
    components: &[String],
    node: Node<'_>,
) -> Result<Unit> {
    let short_name = short_name(node, source)?;
    let kind = match node.kind() {
        "method_declaration" => "method",
        "constructor_declaration" => "constructor",
        "static_initializer" => "static",
        other => other,
    };
    let span = node_span(node);
    let unit_text = source_text(node, source)?;
    let start_line = span.start.row + 1;
    let end_line = span.end.row + 1;

    Ok(Unit {
        id: format!("{relative_path}:{start_line}:{short_name}"),
        kind: String::from(kind),
        name: short_name.clone(),
        short_name,
        long_name: String::new(),
        relative_file_name: String::from(relative_path),
        file_lines_count,
        components: components.to_vec(),
        start_line,
        end_line,
        lines_of_code: end_line - start_line + 1,
        mc_cabe_index: calculate_mcabe_index(&unit_text),
        number_of_parameters: count_parameters(node),
        number_of_literals: 0,
        number_of_statements: 0,
        number_of_expressions: 0,
    })
}

fn short_name(node: Node<'_>, source: &str) -> Result<String> {
    if node.kind() == "static_initializer" {
        return Ok(String::from("static"));
    }

    let parameters = node
        .child_by_field_name("parameters")
        .with_context(|| format!("{} is missing parameter metadata", node.kind()))?;
    let header = source
        .get(node.start_byte()..parameters.start_byte())
        .with_context(|| format!("slice Java declaration header for {}", node.kind()))?;

    Ok(format!("{}()", normalize_whitespace(header)))
}

fn count_parameters(node: Node<'_>) -> usize {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return 0;
    };

    let mut cursor = parameters.walk();
    parameters.named_children(&mut cursor).count()
}

fn calculate_mcabe_index(unit_text: &str) -> usize {
    let normalized = format!(
        " {} ",
        unit_text
            .replace('\n', " ")
            .replace('(', " (")
            .replace('{', " {")
    );
    let literals = [
        " if ", " while ", " for ", " case ", "&&", "||", " ? ", " catch ",
    ];

    1 + literals
        .iter()
        .map(|literal| normalized.matches(literal).count())
        .sum::<usize>()
}

fn collect_dependency_anchors(inputs: &[JavaSourceInput]) -> BTreeMap<String, DependencyAnchor> {
    let mut anchors = BTreeMap::new();

    for input in inputs {
        let Some(anchor) = extract_package_anchor(input) else {
            continue;
        };

        anchors
            .entry(anchor.anchor.clone())
            .and_modify(|existing| merge_dependency_anchor(existing, &anchor))
            .or_insert(anchor);
    }

    anchors
}

fn extract_package_dependencies(
    inputs: &[JavaSourceInput],
    anchors: &BTreeMap<String, DependencyAnchor>,
) -> Result<Vec<Dependency>> {
    let matchers = anchors
        .values()
        .map(AnchorMatcher::new)
        .collect::<Result<Vec<_>>>()?;
    let mut dependencies = BTreeMap::<(String, String), Dependency>::new();

    for input in inputs {
        let Some((source_anchor_name, _)) = package_declaration(&input.source) else {
            continue;
        };
        let Some(source_anchor) = anchors.get(&source_anchor_name).cloned() else {
            continue;
        };
        let lines = normalized_dependency_lines(&input.source);

        for matcher in &matchers {
            if matcher.anchor.anchor == source_anchor_name {
                continue;
            }

            let Some(code_fragment) = matcher.dependency_code_fragment(&lines) else {
                continue;
            };

            let entry = dependencies
                .entry((source_anchor_name.clone(), matcher.anchor.anchor.clone()))
                .or_insert_with(|| Dependency {
                    from: source_anchor_name.clone(),
                    to: matcher.anchor.anchor.clone(),
                    kind: String::from("package"),
                    from_anchor: source_anchor.clone(),
                    to_anchor: matcher.anchor.clone(),
                    from_files: Vec::new(),
                });

            entry.from_files.push(SourceFileDependency {
                file: input.file.clone(),
                code_fragment,
            });
        }
    }

    let mut dependencies = dependencies.into_values().collect::<Vec<_>>();
    dependencies.sort_by(|left, right| (&left.from, &left.to).cmp(&(&right.from, &right.to)));

    Ok(dependencies)
}

fn extract_component_dependencies(dependencies: &[Dependency]) -> Vec<ComponentDependency> {
    let decompositions = decomposition_names(dependencies);
    let mut component_dependencies = Vec::new();

    for decomposition in decompositions {
        component_dependencies.extend(extract_component_dependencies_for_decomposition(
            dependencies,
            &decomposition,
        ));
    }

    component_dependencies.sort_by(|left, right| {
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
    component_dependencies
}

fn extract_component_dependencies_for_decomposition(
    dependencies: &[Dependency],
    decomposition: &str,
) -> Vec<ComponentDependency> {
    let mut file_to_target_component_links = BTreeSet::new();
    let mut component_dependencies = BTreeMap::<(String, String), ComponentDependency>::new();

    for dependency in dependencies {
        let target_components = component_names(
            &dependency
                .to_anchor
                .files
                .iter()
                .flat_map(|file| file.components.clone())
                .collect::<Vec<_>>(),
            decomposition,
        );

        if target_components.is_empty() {
            continue;
        }

        for source_file_dependency in &dependency.from_files {
            let source_components =
                component_names(&source_file_dependency.file.components, decomposition);

            if source_components.is_empty()
                || source_components
                    .iter()
                    .any(|source_component| target_components.contains(source_component))
            {
                continue;
            }

            for target_component in &target_components {
                if !file_to_target_component_links.insert((
                    source_file_dependency.file.relative_path.clone(),
                    target_component.clone(),
                )) {
                    continue;
                }

                for source_component in &source_components {
                    let entry = component_dependencies
                        .entry((source_component.clone(), target_component.clone()))
                        .or_insert_with(|| ComponentDependency {
                            decomposition: String::from(decomposition),
                            from_component: source_component.clone(),
                            to_component: target_component.clone(),
                            ..ComponentDependency::default()
                        });

                    entry.count += 1;
                    entry.loc_from += source_file_dependency.file.lines_of_code;
                    entry.evidence.push(DependencyEvidence {
                        path_from: source_file_dependency.file.relative_path.clone(),
                        evidence: source_file_dependency.code_fragment.clone(),
                    });
                }
            }
        }
    }

    component_dependencies.into_values().collect()
}

fn decomposition_names(dependencies: &[Dependency]) -> BTreeSet<String> {
    dependencies
        .iter()
        .flat_map(|dependency| {
            dependency
                .from_files
                .iter()
                .flat_map(|file| file.file.components.iter())
                .chain(
                    dependency
                        .to_anchor
                        .files
                        .iter()
                        .flat_map(|file| file.components.iter()),
                )
        })
        .filter_map(|component| {
            component
                .split_once("::")
                .map(|(decomposition, _)| decomposition.to_string())
        })
        .collect()
}

fn component_names(components: &[String], decomposition: &str) -> Vec<String> {
    components
        .iter()
        .filter_map(|component| {
            let (component_decomposition, component_name) = component.split_once("::")?;
            (component_decomposition == decomposition).then(|| component_name.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn extract_package_anchor(input: &JavaSourceInput) -> Option<DependencyAnchor> {
    let (package_name, code_fragment) = package_declaration(&input.source)?;

    Some(DependencyAnchor {
        anchor: package_name.clone(),
        code_fragment,
        dependency_patterns: vec![format!(
            "import.* {}([.][A-Z].*|[.][*]|);",
            package_name.replace('.', "[.]")
        )],
        files: vec![input.file.clone()],
    })
}

fn package_declaration(source: &str) -> Option<(String, String)> {
    let normalized = normalized_dependency_content(source);
    let start = normalized.find(JAVA_PACKAGE_PREFIX)?;
    let package_name_start = start + JAVA_PACKAGE_PREFIX.len();
    let package_name_end = package_name_start + normalized[package_name_start..].find(';')?;
    let package_name = normalized[package_name_start..package_name_end]
        .trim()
        .to_string();

    if package_name.is_empty() {
        return None;
    }

    let code_fragment = normalized[start..=package_name_end].trim().to_string();
    Some((package_name, code_fragment))
}

fn merge_dependency_anchor(existing: &mut DependencyAnchor, candidate: &DependencyAnchor) {
    for file in &candidate.files {
        if existing
            .files
            .iter()
            .all(|existing_file| existing_file.relative_path != file.relative_path)
        {
            existing.files.push(file.clone());
        }
    }
}

fn normalized_dependency_content(content: &str) -> String {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', " ")
}

fn normalized_dependency_lines(content: &str) -> Vec<String> {
    normalized_dependency_content(content)
        .lines()
        .map(limit_regex_content)
        .collect()
}

fn limit_regex_content(content: &str) -> String {
    content.chars().take(MAX_REGEX_CONTENT_LENGTH).collect()
}

fn cleaned_lines_of_code(source: &str) -> usize {
    strip_comments_preserving_line_breaks(&normalized_dependency_content(source))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

fn strip_comments_preserving_line_breaks(source: &str) -> String {
    let characters = source.chars().collect::<Vec<_>>();
    let mut result = String::with_capacity(source.len());
    let mut index = 0;
    let mut state = CleanerState::Normal;

    while index < characters.len() {
        let current = characters[index];
        let next = characters.get(index + 1).copied();

        match state {
            CleanerState::Normal => match (current, next) {
                ('/', Some('/')) => {
                    state = CleanerState::LineComment;
                    index += 1;
                }
                ('/', Some('*')) => {
                    state = CleanerState::BlockComment;
                    index += 1;
                }
                ('"', _) => {
                    state = CleanerState::DoubleQuotedString;
                    result.push(current);
                }
                ('\'', _) => {
                    state = CleanerState::SingleQuotedString;
                    result.push(current);
                }
                _ => result.push(current),
            },
            CleanerState::LineComment => {
                if current == '\n' {
                    result.push(current);
                    state = CleanerState::Normal;
                }
            }
            CleanerState::BlockComment => {
                if current == '\n' {
                    result.push(current);
                } else if current == '*' && next == Some('/') {
                    state = CleanerState::Normal;
                    index += 1;
                }
            }
            CleanerState::DoubleQuotedString => {
                result.push(current);

                if current == '\\' {
                    if let Some(escaped) = next {
                        result.push(escaped);
                        index += 1;
                    }
                } else if current == '"' {
                    state = CleanerState::Normal;
                }
            }
            CleanerState::SingleQuotedString => {
                result.push(current);

                if current == '\\' {
                    if let Some(escaped) = next {
                        result.push(escaped);
                        index += 1;
                    }
                } else if current == '\'' {
                    state = CleanerState::Normal;
                }
            }
        }

        index += 1;
    }

    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanerState {
    Normal,
    LineComment,
    BlockComment,
    DoubleQuotedString,
    SingleQuotedString,
}

#[derive(Debug)]
struct AnchorMatcher {
    anchor: DependencyAnchor,
    patterns: Vec<Regex>,
}

impl AnchorMatcher {
    fn new(anchor: &DependencyAnchor) -> Result<Self> {
        let patterns = anchor
            .dependency_patterns
            .iter()
            .map(|pattern| Regex::new(&format!("^(?:{pattern})$")))
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("compile dependency patterns for {}", anchor.anchor))?;

        Ok(Self {
            anchor: anchor.clone(),
            patterns,
        })
    }

    fn dependency_code_fragment(&self, lines: &[String]) -> Option<String> {
        lines
            .iter()
            .find(|line| self.patterns.iter().any(|pattern| pattern.is_match(line)))
            .cloned()
    }
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn source_text(node: Node<'_>, source: &str) -> Result<String> {
    source
        .get(node.start_byte()..node.end_byte())
        .map(str::to_owned)
        .with_context(|| format!("slice Java node {}", node.kind()))
}

#[cfg(test)]
mod tests {
    use super::{JavaSourceInput, analyze_file, analyze_repository};
    use anyhow::{Context, Result};
    use sokrates_ir::FileRecord;
    use std::fs;
    use std::path::PathBuf;

    fn fixture_java_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("java-units-sample")
            .join("input")
            .join("src")
            .join("main")
            .join("java")
            .join("app")
            .join("Calculator.java")
    }

    fn dependency_fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("java-dependencies-sample")
            .join("input")
    }

    fn load_dependency_fixture(relative_path: &str) -> Result<JavaSourceInput> {
        let source =
            fs::read_to_string(dependency_fixture_root().join(relative_path.replace('/', "\\")))
                .with_context(|| format!("read dependency fixture {relative_path}"))?;
        let component = relative_path
            .split('/')
            .next()
            .context("derive component from dependency fixture path")?;

        Ok(JavaSourceInput {
            relative_path: String::from(relative_path),
            file: FileRecord {
                relative_path: String::from(relative_path),
                extension: String::from("java"),
                lines_of_code: source.lines().count(),
                components: vec![format!("primary::{component}")],
                concerns: Vec::new(),
                units_count: 0,
                units_mc_cabe_index_sum: 0,
                lines_of_code_in_units: 0,
            },
            source,
        })
    }

    #[test]
    fn extracts_java_units_matching_current_fixture_expectations() -> Result<()> {
        let relative_path = "src/main/java/app/Calculator.java";
        let source = fs::read_to_string(fixture_java_path()).context("read Java units fixture")?;
        let analysis = analyze_file(relative_path, &source, vec![String::from("primary::src")])?;

        let summary = analysis
            .units
            .iter()
            .map(|unit| {
                (
                    unit.short_name.as_str(),
                    unit.start_line,
                    unit.end_line,
                    unit.lines_of_code,
                    unit.mc_cabe_index,
                    unit.number_of_parameters,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            summary,
            vec![
                ("public String classify()", 15, 22, 8, 4, 1),
                ("static", 4, 6, 3, 1, 0),
                ("public int add()", 11, 13, 3, 1, 2),
                ("public Calculator()", 8, 9, 2, 1, 0),
            ]
        );
        assert!(analysis.diagnostics.is_empty());

        Ok(())
    }

    #[test]
    fn calculates_file_level_java_metrics_matching_current_fixture_expectations() -> Result<()> {
        let relative_path = "src/main/java/app/Calculator.java";
        let source = fs::read_to_string(fixture_java_path()).context("read Java units fixture")?;
        let analysis = analyze_repository(&[JavaSourceInput {
            relative_path: String::from(relative_path),
            file: FileRecord {
                relative_path: String::from(relative_path),
                extension: String::from("java"),
                lines_of_code: 0,
                components: vec![String::from("primary::src")],
                concerns: Vec::new(),
                units_count: 0,
                units_mc_cabe_index_sum: 0,
                lines_of_code_in_units: 0,
            },
            source,
        }])?;

        assert_eq!(
            analysis
                .files
                .iter()
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
    fn extracts_java_package_and_component_dependencies_matching_fixture_expectations() -> Result<()>
    {
        let analysis = analyze_repository(&[
            load_dependency_fixture("alpha/api/AlphaService.java")?,
            load_dependency_fixture("alpha/internal/AlphaHelper.java")?,
            load_dependency_fixture("beta/api/BetaFacade.java")?,
            load_dependency_fixture("beta/api/BetaService.java")?,
            load_dependency_fixture("beta/internal/BetaHelper.java")?,
        ])?;

        let dependency_summary = analysis
            .dependencies
            .iter()
            .map(|dependency| {
                (
                    dependency.from.as_str(),
                    dependency.to.as_str(),
                    dependency
                        .from_files
                        .iter()
                        .map(|from_file| {
                            (
                                from_file.file.relative_path.as_str(),
                                from_file.code_fragment.as_str(),
                            )
                        })
                        .collect::<Vec<_>>(),
                    dependency
                        .to_anchor
                        .files
                        .iter()
                        .map(|file| file.relative_path.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            dependency_summary,
            vec![
                (
                    "alpha.api",
                    "alpha.internal",
                    vec![(
                        "alpha/api/AlphaService.java",
                        "import alpha.internal.AlphaHelper;"
                    )],
                    vec!["alpha/internal/AlphaHelper.java"],
                ),
                (
                    "alpha.api",
                    "beta.api",
                    vec![(
                        "alpha/api/AlphaService.java",
                        "import beta.api.BetaService;"
                    )],
                    vec!["beta/api/BetaFacade.java", "beta/api/BetaService.java"],
                ),
                (
                    "alpha.api",
                    "beta.internal",
                    vec![(
                        "alpha/api/AlphaService.java",
                        "import beta.internal.BetaHelper;"
                    )],
                    vec!["beta/internal/BetaHelper.java"],
                ),
            ]
        );

        let component_summary = analysis
            .component_dependencies
            .iter()
            .map(|dependency| {
                (
                    dependency.decomposition.as_str(),
                    dependency.from_component.as_str(),
                    dependency.to_component.as_str(),
                    dependency.count,
                    dependency.loc_from,
                    dependency
                        .evidence
                        .iter()
                        .map(|evidence| (evidence.path_from.as_str(), evidence.evidence.as_str()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            component_summary,
            vec![(
                "primary",
                "alpha",
                "beta",
                1,
                11,
                vec![(
                    "alpha/api/AlphaService.java",
                    "import beta.api.BetaService;"
                )],
            )]
        );
        assert_eq!(
            analysis
                .files
                .iter()
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
