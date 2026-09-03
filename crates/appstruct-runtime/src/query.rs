// Shared list-query bounds and LIKE pattern escaping.

/// Maximum accepted offset-pagination page number.
pub const MAX_LIST_PAGE: u64 = 10_000;
/// Maximum accepted offset-pagination page size.
pub const MAX_LIST_PAGE_SIZE: u64 = 100;

/// Returns whether offset pagination arguments are within the generated API bounds.
#[must_use]
pub const fn list_page_is_valid(page: u64, page_size: u64) -> bool {
    page >= 1 && page <= MAX_LIST_PAGE && page_size >= 1 && page_size <= MAX_LIST_PAGE_SIZE
}

/// Builds a SQL `LIKE` pattern for substring search with `%`, `_`, and `\` escaped.
#[must_use]
pub fn like_contains_pattern(term: &str) -> String {
    let mut pattern = String::from("%");
    for character in term.chars() {
        if matches!(character, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::{MAX_LIST_PAGE, MAX_LIST_PAGE_SIZE, like_contains_pattern, list_page_is_valid};

    #[test]
    fn list_pages_are_bounded() {
        assert!(list_page_is_valid(1, 25));
        assert!(list_page_is_valid(MAX_LIST_PAGE, MAX_LIST_PAGE_SIZE));
        assert!(!list_page_is_valid(0, 25));
        assert!(!list_page_is_valid(MAX_LIST_PAGE + 1, 25));
        assert!(!list_page_is_valid(1, 0));
        assert!(!list_page_is_valid(1, MAX_LIST_PAGE_SIZE + 1));
    }

    #[test]
    fn like_contains_escapes_wildcards_and_backslashes() {
        assert_eq!(like_contains_pattern("ab"), "%ab%");
        assert_eq!(like_contains_pattern(""), "%%");
        assert_eq!(like_contains_pattern(r"a%b_c\d"), r"%a\%b\_c\\d%");
    }
}
