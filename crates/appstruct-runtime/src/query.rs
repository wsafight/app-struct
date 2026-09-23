// Shared list-query bounds and LIKE pattern escaping.

/// Maximum accepted offset-pagination page number.
pub const MAX_LIST_PAGE: u64 = 10_000;
/// Maximum accepted offset-pagination page size.
pub const MAX_LIST_PAGE_SIZE: u64 = 100;
/// Maximum Unicode scalar values accepted for a resource search term.
pub const MAX_SEARCH_CHARS: usize = 200;

/// Returns whether offset pagination arguments are within the generated API bounds.
#[must_use]
pub const fn list_page_is_valid(page: u64, page_size: u64) -> bool {
    page >= 1 && page <= MAX_LIST_PAGE && page_size >= 1 && page_size <= MAX_LIST_PAGE_SIZE
}

/// Returns whether a resource search term fits the generated API bound.
#[must_use]
pub fn search_term_is_valid(term: &str) -> bool {
    term.chars().count() <= MAX_SEARCH_CHARS
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
    use super::{
        MAX_LIST_PAGE, MAX_LIST_PAGE_SIZE, MAX_SEARCH_CHARS, like_contains_pattern,
        list_page_is_valid, search_term_is_valid,
    };

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

    #[test]
    fn search_terms_are_bounded_by_unicode_characters() {
        assert!(search_term_is_valid(&"a".repeat(MAX_SEARCH_CHARS)));
        assert!(!search_term_is_valid(&"a".repeat(MAX_SEARCH_CHARS + 1)));
        assert!(search_term_is_valid(&"界".repeat(MAX_SEARCH_CHARS)));
        assert!(!search_term_is_valid(&"界".repeat(MAX_SEARCH_CHARS + 1)));
    }
}
