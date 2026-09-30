use appstruct_ir::{AppIr, Diagnostic, SourceSpan};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, Expression, ObjectPropertyKind, PropertyKey, Statement,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const REGISTRY_CANDIDATES: &[&str] = &[
    "app/web/registry.ts",
    "app/web/registry.tsx",
    "app/web/registry/index.ts",
    "app/web/registry/index.tsx",
];

/// Symbols the user must provide under `app/web/` for the App Spec to build.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequiredSymbols {
    pub field_components: BTreeSet<String>,
    pub page_components: BTreeSet<String>,
}

impl RequiredSymbols {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.field_components.is_empty() && self.page_components.is_empty()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ProvidedSymbols {
    field_components: BTreeSet<String>,
    page_components: BTreeSet<String>,
}

enum RegistryScan {
    Conclusive {
        provided: ProvidedSymbols,
        span: Span,
    },
    Inconclusive,
}

/// Collects every custom component referenced by the App Spec.
#[must_use]
pub fn required_symbols(ir: &AppIr) -> RequiredSymbols {
    let field_components = ir
        .entities
        .iter()
        .flat_map(|entity| &entity.fields)
        .filter_map(|field| field.ui_component.clone())
        .collect();
    let page_components = ir.pages.iter().map(|page| page.component.clone()).collect();
    RequiredSymbols {
        field_components,
        page_components,
    }
}

/// Parses the registry module imported by the generated entry point and reports missing keys.
///
/// Syntax errors, re-exports, and dynamic registry shapes make the cheap check inconclusive. The
/// build stage remains authoritative because it runs TypeScript module resolution and type checking.
///
/// # Errors
///
/// Returns an error when an existing registry module cannot be read.
pub fn check_user_symbols(project: &Path, ir: &AppIr) -> Result<Vec<Diagnostic>, std::io::Error> {
    let required = required_symbols(ir);
    if required.is_empty() {
        return Ok(Vec::new());
    }
    let Some(path) = registry_path(project) else {
        let span = fallback_span(project);
        return Ok(missing_diagnostics(
            ir,
            &required,
            &ProvidedSymbols::default(),
            &span,
        ));
    };
    let source = fs::read_to_string(&path)?;
    let RegistryScan::Conclusive { provided, span } = scan_registry(&path, &source) else {
        return Ok(Vec::new());
    };
    let span = source_span(project, &path, &source, span);
    Ok(missing_diagnostics(ir, &required, &provided, &span))
}

fn registry_path(project: &Path) -> Option<PathBuf> {
    REGISTRY_CANDIDATES
        .iter()
        .map(|path| project.join(path))
        .find(|path| path.is_file())
}

fn scan_registry(path: &Path, source: &str) -> RegistryScan {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::tsx());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return RegistryScan::Inconclusive;
    }
    let mut initializer = None;
    let mut local_export = None;
    for statement in &parsed.program.body {
        match statement {
            Statement::ExportAllDeclaration(_) => {
                return RegistryScan::Inconclusive;
            }
            Statement::ExportFromDeclaration(export)
                if export
                    .specifiers
                    .iter()
                    .any(|specifier| specifier.exported.name().as_str() == "registry") =>
            {
                return RegistryScan::Inconclusive;
            }
            Statement::ExportDeclaration(export) => {
                if let Some(expression) = declaration_initializer(&export.declaration, "registry")
                    && initializer.replace(expression).is_some()
                {
                    return RegistryScan::Inconclusive;
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                for specifier in &export.specifiers {
                    if specifier.exported.name().as_str() == "registry" {
                        local_export = Some(specifier.local.name().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if initializer.is_none()
        && let Some(local) = local_export
    {
        initializer = parsed
            .program
            .body
            .iter()
            .find_map(|statement| statement_initializer(statement, &local));
        if initializer.is_none() {
            return RegistryScan::Inconclusive;
        }
    }
    let Some(initializer) = initializer else {
        return RegistryScan::Conclusive {
            provided: ProvidedSymbols::default(),
            span: Span::new(0, 0),
        };
    };
    let span = initializer.span();
    let Some(provided) = collect_registry(initializer) else {
        return RegistryScan::Inconclusive;
    };
    RegistryScan::Conclusive { provided, span }
}

fn statement_initializer<'a>(
    statement: &'a Statement<'a>,
    name: &str,
) -> Option<&'a Expression<'a>> {
    match statement {
        Statement::VariableDeclaration(declaration) => variable_initializer(declaration, name),
        _ => None,
    }
}

fn declaration_initializer<'a>(
    declaration: &'a Declaration<'a>,
    name: &str,
) -> Option<&'a Expression<'a>> {
    match declaration {
        Declaration::VariableDeclaration(declaration) => variable_initializer(declaration, name),
        _ => None,
    }
}

fn variable_initializer<'a>(
    declaration: &'a oxc_ast::ast::VariableDeclaration<'a>,
    name: &str,
) -> Option<&'a Expression<'a>> {
    declaration.declarations.iter().find_map(|declarator| {
        let BindingPattern::BindingIdentifier(identifier) = &declarator.id else {
            return None;
        };
        (identifier.name.as_str() == name)
            .then_some(declarator.init.as_ref())
            .flatten()
    })
}

fn collect_registry(expression: &Expression<'_>) -> Option<ProvidedSymbols> {
    let expression = unwrap_registry_expression(expression)?;
    let Expression::ObjectExpression(object) = expression else {
        return None;
    };
    let mut provided = ProvidedSymbols::default();
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        let name = static_property_name(&property.key)?;
        match name {
            "fields" => {
                provided.field_components = collect_component_keys(&property.value)?;
            }
            "pages" => {
                provided.page_components = collect_component_keys(&property.value)?;
            }
            _ => {}
        }
    }
    Some(provided)
}

fn unwrap_registry_expression<'a>(expression: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let expression = expression.get_inner_expression();
    let Expression::CallExpression(call) = expression else {
        return Some(expression);
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name.as_str() != "defineAppStructRegistry" {
        return None;
    }
    let argument = call.arguments.first()?.as_expression()?;
    Some(argument.get_inner_expression())
}

fn collect_component_keys(expression: &Expression<'_>) -> Option<BTreeSet<String>> {
    let Expression::ObjectExpression(object) = expression.get_inner_expression() else {
        return None;
    };
    let mut names = BTreeSet::new();
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };
        names.insert(static_property_name(&property.key)?.to_owned());
    }
    Some(names)
}

fn static_property_name<'key>(key: &'key PropertyKey<'_>) -> Option<&'key str> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.as_str()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

fn missing_diagnostics(
    ir: &AppIr,
    required: &RequiredSymbols,
    provided: &ProvidedSymbols,
    span: &SourceSpan,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for page in &ir.pages {
        if required.page_components.contains(&page.component)
            && !provided.page_components.contains(&page.component)
        {
            diagnostics.push(Diagnostic::error(
                "AS3106",
                format!(
                    "page `{}` references component `{}`, which is missing from `registry.pages`",
                    page.id, page.component
                ),
                span.clone(),
            ));
        }
    }
    for entity in &ir.entities {
        for field in &entity.fields {
            let Some(component) = &field.ui_component else {
                continue;
            };
            if !provided.field_components.contains(component) {
                diagnostics.push(Diagnostic::error(
                    "AS3107",
                    format!(
                        "field `{}` references UI component `{component}`, which is missing from `registry.fields`",
                        field.id
                    ),
                    span.clone(),
                ));
            }
        }
    }
    diagnostics
}

fn fallback_span(project: &Path) -> SourceSpan {
    let path = project.join("appstruct.yaml");
    SourceSpan {
        file: relative_path(project, &path),
        start: 0,
        end: 0,
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 1,
    }
}

fn source_span(project: &Path, path: &Path, source: &str, span: Span) -> SourceSpan {
    let start = span.start as usize;
    let end = span.end as usize;
    let (line, column) = line_column(source, start);
    let (end_line, end_column) = line_column(source, end);
    SourceSpan {
        file: relative_path(project, path),
        start,
        end,
        line,
        column,
        end_line,
        end_column,
    }
}

fn relative_path(project: &Path, path: &Path) -> String {
    path.strip_prefix(project)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, tail)| tail)
        .chars()
        .count()
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests;
