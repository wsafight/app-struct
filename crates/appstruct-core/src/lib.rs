//! Pure algorithms and shared resource protocols for `AppStruct`.

mod hash;
pub mod naming;
pub mod query;
pub mod resource;

#[doc(hidden)]
pub mod __source {
    pub const LIB: &str = include_str!("lib.rs");
    pub const HASH: &str = include_str!("hash.rs");
    pub const NAMING: &str = include_str!("naming.rs");
    pub const QUERY: &str = include_str!("query.rs");
    pub const RESOURCE: &str = include_str!("resource.rs");
}

pub use hash::{sha256_hex, sha256_tagged};
pub use naming::{
    is_app_name, is_rust_field_name, is_rust_type_name, is_sql_name, pluralize, to_snake_case,
};
pub use query::{
    MAX_LIST_PAGE, MAX_LIST_PAGE_SIZE, MAX_SEARCH_CHARS, like_contains_pattern, list_page_is_valid,
    search_term_is_valid,
};
pub use resource::{
    BulkDeleteInput, BulkFailure, BulkResult, BulkUpdateInput, CSV_EXPORT_PAGE_SIZE, CsvError,
    ListMeta, ListQuery, ListResponse, MAX_BULK_ITEMS, MAX_CSV_EXPORT_ROWS, MAX_CSV_IMPORT_ROWS,
    bulk_failure, bulk_request_size_is_valid, csv_cell, csv_escape, csv_json_value, decode_cursor,
    encode_cursor, parse_csv_rows, parse_revision_etag, revision_etag,
};
