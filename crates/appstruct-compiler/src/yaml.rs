use appstruct_ir::{Diagnostic, SourceSpan};
use saphyr_parser::{Event, Parser, ScalarStyle, Span};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub kind: NodeKind,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub(crate) enum NodeKind {
    Scalar { value: String, plain: bool },
    Sequence(Vec<Node>),
    Mapping(BTreeMap<String, MappingEntry>),
}

#[derive(Clone, Debug)]
pub(crate) struct MappingEntry {
    pub key_span: SourceSpan,
    pub value: Node,
}

impl Node {
    pub fn mapping(&self) -> Option<&BTreeMap<String, MappingEntry>> {
        match &self.kind {
            NodeKind::Mapping(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn sequence(&self) -> Option<&[Node]> {
        match &self.kind {
            NodeKind::Sequence(items) => Some(items),
            _ => None,
        }
    }

    pub fn scalar(&self) -> Option<(&str, bool)> {
        match &self.kind {
            NodeKind::Scalar { value, plain } => Some((value, *plain)),
            _ => None,
        }
    }
}

pub(crate) fn parse(file: &str, source: &str) -> Result<Node, Diagnostic> {
    let events = Parser::new_from_str(source)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            let marker = error.marker();
            Diagnostic::error(
                "AS1001",
                format!("invalid YAML: {}", error.info()),
                marker_span(file, source, marker.index(), marker.line(), marker.col()),
            )
        })?;

    let mut parser = AstParser {
        file,
        source,
        events,
        cursor: 0,
    };
    parser.document()
}

struct AstParser<'source> {
    file: &'source str,
    source: &'source str,
    events: Vec<(Event<'source>, Span)>,
    cursor: usize,
}

impl AstParser<'_> {
    fn document(&mut self) -> Result<Node, Diagnostic> {
        self.expect_event(|event| matches!(event, Event::StreamStart), "YAML stream")?;
        self.expect_event(
            |event| matches!(event, Event::DocumentStart(_)),
            "YAML document",
        )?;

        let node = self.node()?;
        self.expect_event(
            |event| matches!(event, Event::DocumentEnd),
            "end of document",
        )?;

        if matches!(self.peek_event(), Some(Event::DocumentStart(_))) {
            return Err(self.error_at_cursor("AS1002", "multiple YAML documents are not supported"));
        }

        self.expect_event(|event| matches!(event, Event::StreamEnd), "end of stream")?;
        Ok(node)
    }

    fn node(&mut self) -> Result<Node, Diagnostic> {
        let Some((event, span)) = self.events.get(self.cursor).cloned() else {
            return Err(self.error_at_cursor("AS1001", "expected a YAML value"));
        };
        self.cursor += 1;

        match event {
            Event::Alias(_) => Err(Diagnostic::error(
                "AS1003",
                "YAML aliases are not supported",
                self.source_span(span),
            )
            .with_help("write the value explicitly instead of using an alias")),
            Event::Scalar(value, style, anchor, _) => {
                self.reject_anchor(anchor, span)?;
                Ok(Node {
                    kind: NodeKind::Scalar {
                        value: value.into_owned(),
                        plain: style == ScalarStyle::Plain,
                    },
                    span: self.source_span(span),
                })
            }
            Event::SequenceStart(anchor, _) => {
                self.reject_anchor(anchor, span)?;
                let start = span.start;
                let mut items = Vec::new();
                while !matches!(self.peek_event(), Some(Event::SequenceEnd)) {
                    items.push(self.node()?);
                }
                let end = self.take_end(|event| matches!(event, Event::SequenceEnd))?;
                Ok(Node {
                    kind: NodeKind::Sequence(items),
                    span: self.source_span(Span::new(start, end.end)),
                })
            }
            Event::MappingStart(anchor, _) => {
                self.reject_anchor(anchor, span)?;
                let start = span.start;
                let mut entries = BTreeMap::new();
                while !matches!(self.peek_event(), Some(Event::MappingEnd)) {
                    let key = self.node()?;
                    let Some((key_value, _)) = key.scalar() else {
                        return Err(Diagnostic::error(
                            "AS1004",
                            "mapping keys must be scalar strings",
                            key.span,
                        ));
                    };
                    let key_value = key_value.to_owned();
                    if key_value == "<<" {
                        return Err(Diagnostic::error(
                            "AS1003",
                            "YAML merge keys are not supported",
                            key.span,
                        )
                        .with_help("write each mapping entry explicitly"));
                    }
                    let value = self.node()?;
                    if let Some(previous) = entries.insert(
                        key_value.clone(),
                        MappingEntry {
                            key_span: key.span.clone(),
                            value,
                        },
                    ) {
                        return Err(Diagnostic::error(
                            "AS1005",
                            format!("duplicate mapping key `{key_value}`"),
                            key.span,
                        )
                        .with_secondary(previous.key_span, "first declared here"));
                    }
                }
                let end = self.take_end(|event| matches!(event, Event::MappingEnd))?;
                Ok(Node {
                    kind: NodeKind::Mapping(entries),
                    span: self.source_span(Span::new(start, end.end)),
                })
            }
            _ => Err(Diagnostic::error(
                "AS1001",
                "expected a YAML scalar, sequence, or mapping",
                self.source_span(span),
            )),
        }
    }

    fn reject_anchor(&self, anchor: usize, span: Span) -> Result<(), Diagnostic> {
        if anchor == 0 {
            return Ok(());
        }
        Err(Diagnostic::error(
            "AS1003",
            "YAML anchors are not supported",
            self.source_span(span),
        )
        .with_help("write the value explicitly instead of using an anchor"))
    }

    fn expect_event(
        &mut self,
        predicate: impl FnOnce(&Event<'_>) -> bool,
        expected: &str,
    ) -> Result<Span, Diagnostic> {
        let Some((event, span)) = self.events.get(self.cursor) else {
            return Err(self.error_at_cursor("AS1001", format!("expected {expected}")));
        };
        if !predicate(event) {
            return Err(Diagnostic::error(
                "AS1001",
                format!("expected {expected}"),
                self.source_span(*span),
            ));
        }
        self.cursor += 1;
        Ok(*span)
    }

    fn take_end(&mut self, predicate: impl FnOnce(&Event<'_>) -> bool) -> Result<Span, Diagnostic> {
        self.expect_event(predicate, "collection end")
    }

    fn peek_event(&self) -> Option<&Event<'_>> {
        self.events.get(self.cursor).map(|(event, _)| event)
    }

    fn error_at_cursor(&self, code: &str, message: impl Into<String>) -> Diagnostic {
        let span = self.events.get(self.cursor).map_or_else(
            || eof_span(self.file, self.source),
            |(_, span)| self.source_span(*span),
        );
        Diagnostic::error(code, message, span)
    }

    fn source_span(&self, span: Span) -> SourceSpan {
        SourceSpan {
            file: self.file.to_owned(),
            start: char_to_byte(self.source, span.start.index()),
            end: char_to_byte(self.source, span.end.index()),
            line: span.start.line(),
            column: span.start.col() + 1,
            end_line: span.end.line(),
            end_column: span.end.col() + 1,
        }
    }
}

fn marker_span(file: &str, source: &str, index: usize, line: usize, column: usize) -> SourceSpan {
    let byte = char_to_byte(source, index);
    SourceSpan {
        file: file.to_owned(),
        start: byte,
        end: byte,
        line,
        column: column + 1,
        end_line: line,
        end_column: column + 1,
    }
}

fn eof_span(file: &str, source: &str) -> SourceSpan {
    let (line, column) = source.chars().fold((1, 1), |(line, column), character| {
        if character == '\n' {
            (line + 1, 1)
        } else {
            (line, column + 1)
        }
    });
    SourceSpan {
        file: file.to_owned(),
        start: source.len(),
        end: source.len(),
        line,
        column,
        end_line: line,
        end_column: column,
    }
}

fn char_to_byte(source: &str, char_index: usize) -> usize {
    source
        .char_indices()
        .nth(char_index)
        .map_or(source.len(), |(byte, _)| byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_duplicate_keys_at_second_key() {
        let diagnostic = parse("appstruct.yaml", "name: first\nname: second\n").unwrap_err();
        assert_eq!(diagnostic.code, "AS1005");
        assert_eq!(diagnostic.primary.span.line, 2);
        assert_eq!(diagnostic.secondary[0].span.line, 1);
    }

    #[test]
    fn reports_utf8_offsets_as_bytes() {
        let root = parse("spec/project.yaml", "label: 项目\nname: Project\n").unwrap();
        let entries = root.mapping().unwrap();
        assert_eq!(entries["name"].key_span.start, "label: 项目\n".len());
    }

    #[test]
    fn rejects_aliases() {
        let diagnostic = parse("appstruct.yaml", "one: &value 1\ntwo: *value\n").unwrap_err();
        assert_eq!(diagnostic.code, "AS1003");
    }

    #[test]
    fn rejects_invalid_yaml_and_multiple_documents() {
        let invalid = parse("appstruct.yaml", "foo: [\n").unwrap_err();
        assert_eq!(invalid.code, "AS1001");
        let multiple = parse("appstruct.yaml", "name: one\n---\nname: two\n").unwrap_err();
        assert_eq!(multiple.code, "AS1002");
    }

    #[test]
    fn rejects_anchors_and_merge_keys() {
        let anchor = parse("appstruct.yaml", "one: &label first\n").unwrap_err();
        assert_eq!(anchor.code, "AS1003");
        let merge = parse("appstruct.yaml", "<<: {name: demo}\n").unwrap_err();
        assert_eq!(merge.code, "AS1003");
    }

    #[test]
    fn parses_sequences_and_rejects_non_scalar_keys() {
        let root = parse("appstruct.yaml", "items:\n  - first\n  - second\n").unwrap();
        let items = root.mapping().unwrap()["items"].value.sequence().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].scalar().unwrap().0, "first");
        let nested = parse("appstruct.yaml", "? [not, a, scalar]\n: value\n").unwrap_err();
        assert_eq!(nested.code, "AS1004");
    }

    #[test]
    fn node_accessors_return_none_for_other_kinds() {
        let scalar = parse("appstruct.yaml", "plain\n").unwrap();
        assert!(scalar.mapping().is_none());
        assert!(scalar.sequence().is_none());
        assert_eq!(scalar.scalar().unwrap(), ("plain", true));
        let sequence = parse("appstruct.yaml", "- one\n").unwrap();
        assert!(sequence.mapping().is_none());
        assert!(sequence.scalar().is_none());
        let mapping = parse("appstruct.yaml", "name: demo\n").unwrap();
        assert!(mapping.sequence().is_none());
        assert!(mapping.scalar().is_none());
    }
}
