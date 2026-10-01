use appstruct_contracts::CONTRACT_MATRIX;
use std::{fmt::Write as _, fs, path::Path};

const START: &str = "<!-- contract-matrix:start -->";
const END: &str = "<!-- contract-matrix:end -->";

#[test]
fn compatibility_documentation_matches_contract_constants() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (relative, headers) in [
        ("docs/compatibility.md", ("Contract", "Minimum", "Current")),
        (
            "docs/compatibility.zh-CN.md",
            ("契约", "最低版本", "当前版本"),
        ),
    ] {
        let path = workspace.join(relative);
        let source = fs::read_to_string(&path).unwrap();
        let documented = source
            .split_once(START)
            .and_then(|(_, rest)| rest.split_once(END))
            .map_or_else(
                || panic!("{} is missing contract matrix markers", path.display()),
                |(matrix, _)| matrix.trim(),
            );
        assert_eq!(documented, render_matrix(headers), "{}", path.display());
    }
}

#[test]
fn product_and_design_docs_do_not_restore_obsolete_contract_claims() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in [
        "PRODUCT.md",
        "PRODUCT.en.md",
        "TECHNICAL_DESIGN.md",
        "TECHNICAL_DESIGN.en.md",
    ] {
        let source = fs::read_to_string(workspace.join(relative)).unwrap();
        for obsolete in [
            "IR v7-v11",
            "IR v7-v10 can migrate in memory to v11",
            "400 行上限",
            "at 400 lines",
        ] {
            assert!(
                !source.contains(obsolete),
                "{relative} contains obsolete contract text: {obsolete}"
            );
        }
    }
}

fn render_matrix(headers: (&str, &str, &str)) -> String {
    let mut table = format!(
        "| {} | {} | {} |\n| --- | ---: | ---: |",
        headers.0, headers.1, headers.2
    );
    for (name, range) in CONTRACT_MATRIX {
        write!(
            table,
            "\n| `{name}` | {} | {} |",
            range.minimum, range.current
        )
        .unwrap();
    }
    table
}
