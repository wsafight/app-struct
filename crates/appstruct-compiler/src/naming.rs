pub(crate) fn is_app_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn is_rust_type_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && !is_rust_keyword(value)
}

pub(crate) fn is_rust_field_name(value: &str) -> bool {
    is_sql_name(value) && !is_rust_keyword(value)
}

fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "Self"
            | "abstract"
            | "as"
            | "async"
            | "await"
            | "become"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "final"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "macro"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "override"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

pub(crate) fn is_sql_name(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

pub(crate) fn to_snake_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) fn pluralize(value: &str) -> String {
    if value.ends_with('s') {
        format!("{value}es")
    } else {
        format!("{value}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_type_names() {
        assert_eq!(to_snake_case("Project"), "project");
        assert_eq!(to_snake_case("ProjectMember"), "project_member");
    }

    #[test]
    fn validates_names() {
        assert!(is_app_name("project-hub"));
        assert!(!is_app_name("ProjectHub"));
        assert!(is_rust_type_name("Project"));
        assert!(is_sql_name("project_owner"));
        assert!(!is_rust_field_name("type"));
    }
}
