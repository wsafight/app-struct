use crate::{Artifact, ArtifactKind, generated_header};

pub(super) fn core_artifacts() -> [Artifact; 5] {
    [
        rust_source(
            "backend/core/src/lib.rs",
            crate_source(appstruct_core::__source::LIB),
        ),
        rust_source(
            "backend/core/src/hash.rs",
            module_source(appstruct_core::__source::HASH),
        ),
        rust_source(
            "backend/core/src/naming.rs",
            module_source(appstruct_core::__source::NAMING),
        ),
        rust_source(
            "backend/core/src/query.rs",
            module_source(appstruct_core::__source::QUERY),
        ),
        rust_source(
            "backend/core/src/resource.rs",
            module_source(appstruct_core::__source::RESOURCE),
        ),
    ]
}

pub(super) fn runtime_artifacts() -> [Artifact; 5] {
    [
        rust_source(
            "backend/runtime/src/bigint.rs",
            crate_source(appstruct_runtime::__source::BIGINT),
        ),
        rust_source(
            "backend/runtime/src/lib.rs",
            crate_source(appstruct_runtime::__source::LIB),
        ),
        rust_source(
            "backend/runtime/src/lifecycle.rs",
            module_source(appstruct_runtime::__source::LIFECYCLE),
        ),
        rust_source(
            "backend/runtime/src/origin.rs",
            module_source(appstruct_runtime::__source::ORIGIN),
        ),
        rust_source(
            "backend/runtime/src/supervisor.rs",
            module_source(appstruct_runtime::__source::SUPERVISOR),
        ),
    ]
}

pub(super) fn crate_source(source: &str) -> String {
    let source = source
        .lines()
        .map(|line| {
            line.strip_prefix("//!")
                .map_or(line.to_owned(), |line| format!("//{line}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}{}\n", generated_header("//"), source)
}

fn module_source(source: &str) -> String {
    format!("{}{}", generated_header("//"), source)
}

fn rust_source(path: &str, source: String) -> Artifact {
    Artifact::text(path, source, ArtifactKind::RustSource)
}
