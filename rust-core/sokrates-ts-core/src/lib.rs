use anyhow::{Context, Result};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedLanguage {
    Java,
}

impl SupportedLanguage {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "java" => Some(Self::Java),
            _ => None,
        }
    }

    pub fn grammar(self) -> Language {
        match self {
            Self::Java => tree_sitter_java::LANGUAGE.into(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Java => "java",
        }
    }
}

#[derive(Debug)]
pub struct ParsedDocument {
    language: SupportedLanguage,
    source: String,
    tree: Tree,
}

impl ParsedDocument {
    pub fn language(&self) -> SupportedLanguage {
        self.language
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn root_kind(&self) -> &str {
        self.tree.root_node().kind()
    }

    pub fn has_errors(&self) -> bool {
        self.tree.root_node().has_error()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePosition {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureMatch {
    pub capture_name: String,
    pub kind: String,
    pub text: String,
    pub span: SourceSpan,
}

pub fn new_parser(language: SupportedLanguage) -> Result<Parser> {
    let mut parser = Parser::new();
    let grammar = language.grammar();
    parser
        .set_language(&grammar)
        .with_context(|| format!("configure parser for {}", language.name()))?;

    Ok(parser)
}

pub fn parse(language: SupportedLanguage, source: impl Into<String>) -> Result<ParsedDocument> {
    let source = source.into();
    let mut parser = new_parser(language)?;
    let tree = parser
        .parse(&source, None)
        .with_context(|| format!("parse {} source", language.name()))?;

    Ok(ParsedDocument {
        language,
        source,
        tree,
    })
}

pub fn query_captures(document: &ParsedDocument, query_source: &str) -> Result<Vec<CaptureMatch>> {
    let grammar = document.language.grammar();
    let query = Query::new(&grammar, query_source)
        .with_context(|| format!("compile {} query", document.language.name()))?;
    let mut cursor = QueryCursor::new();
    let capture_names = query.capture_names();
    let source_bytes = document.source.as_bytes();
    let mut results = Vec::new();

    let mut query_matches = cursor.matches(&query, document.tree.root_node(), source_bytes);
    while let Some(query_match) = query_matches.next() {
        for capture in query_match.captures {
            let capture_name = capture_names[capture.index as usize].to_string();
            let kind = capture.node.kind().to_string();
            let text = capture
                .node
                .utf8_text(source_bytes)
                .with_context(|| format!("read query capture {}", capture_name))?
                .to_string();

            results.push(CaptureMatch {
                capture_name,
                kind,
                text,
                span: node_span(capture.node),
            });
        }
    }

    Ok(results)
}

pub fn node_span(node: Node<'_>) -> SourceSpan {
    let start = node.start_position();
    let end = node.end_position();

    SourceSpan {
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start: SourcePosition {
            row: start.row,
            column: start.column,
        },
        end: SourcePosition {
            row: end.row,
            column: end.column,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{SupportedLanguage, parse, query_captures};
    use anyhow::{Context, Result};
    use std::fs;
    use std::path::PathBuf;

    fn fixture_java_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("sample-repo")
            .join("input")
            .join("src")
            .join("main")
            .join("java")
            .join("demo")
            .join("HelloService.java")
    }

    fn fixture_java_source() -> Result<String> {
        fs::read_to_string(fixture_java_path()).context("read Java fixture")
    }

    #[test]
    fn java_extension_maps_to_supported_language() {
        assert_eq!(
            SupportedLanguage::from_extension("JAVA"),
            Some(SupportedLanguage::Java)
        );
        assert_eq!(SupportedLanguage::from_extension("xml"), None);
    }

    #[test]
    fn parses_java_fixture_without_syntax_errors() -> Result<()> {
        let document = parse(SupportedLanguage::Java, fixture_java_source()?)?;

        assert_eq!(document.root_kind(), "program");
        assert!(!document.has_errors());

        Ok(())
    }

    #[test]
    fn java_query_extracts_class_and_method_identifiers() -> Result<()> {
        let document = parse(SupportedLanguage::Java, fixture_java_source()?)?;
        let captures = query_captures(
            &document,
            r#"
            (class_declaration
              name: (identifier) @class.name)

            (method_declaration
              name: (identifier) @method.name)
            "#,
        )?;

        let summary = captures
            .iter()
            .map(|capture| (capture.capture_name.as_str(), capture.text.as_str()))
            .collect::<Vec<_>>();

        assert_eq!(
            summary,
            vec![("class.name", "HelloService"), ("method.name", "greet")]
        );
        assert_eq!(captures[0].span.start.row, 2);
        assert_eq!(captures[1].span.start.row, 3);

        Ok(())
    }
}
