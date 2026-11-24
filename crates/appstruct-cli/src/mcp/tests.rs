use super::*;
use std::io::Cursor;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/m0-project")
}

fn roundtrip(lines: &[Value], default_project: Option<&Path>) -> Vec<Value> {
    let mut input = String::new();
    for line in lines {
        input.push_str(&line.to_string());
        input.push('\n');
    }
    let mut reader = Cursor::new(input.into_bytes());
    let mut output = Vec::new();
    serve_stdio(&mut reader, &mut output, default_project).expect("stdio serve succeeds");
    String::from_utf8(output)
        .expect("utf8 output")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid json-rpc response"))
        .collect()
}

#[test]
fn mcp_initialize_list_resources_and_tools() {
    let responses = roundtrip(
        &[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "resources/read", "params": { "uri": "appstruct://guide/agent" } }),
        ],
        Some(&fixture()),
    );
    // Notification produces no response, so 4 responses total.
    assert_eq!(responses.len(), 4);
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "appstruct");
    assert!(responses[1]["result"]["tools"].as_array().unwrap().len() >= 6);
    assert_eq!(
        responses[2]["result"]["resources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        responses[3]["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("File Ownership Boundaries")
    );
}

#[test]
fn mcp_tools_check_schema_access_and_migrate_plan() {
    let responses = roundtrip(
        &[
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": { "name": "appstruct_schema", "arguments": { "section": "app" } }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 11,
                "method": "tools/call",
                "params": { "name": "appstruct_check", "arguments": {} }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 12,
                "method": "tools/call",
                "params": {
                    "name": "appstruct_access_preview",
                    "arguments": { "authenticated": true }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 13,
                "method": "tools/call",
                "params": { "name": "appstruct_migrate_plan", "arguments": {} }
            }),
        ],
        Some(&fixture()),
    );
    assert_eq!(responses.len(), 4);
    assert_eq!(responses[0]["result"]["isError"], false);
    assert_eq!(responses[1]["result"]["isError"], false);
    assert_eq!(responses[1]["result"]["structuredContent"]["valid"], true);
    assert_eq!(responses[2]["result"]["isError"], false);
    assert_ne!(
        responses[2]["result"]["structuredContent"]["rows"]
            .as_array()
            .unwrap()
            .as_slice(),
        &[] as &[Value]
    );
    assert_eq!(responses[3]["result"]["isError"], false);
}

#[test]
fn mcp_rejects_unknown_method_and_invalid_json() {
    let mut reader =
        Cursor::new(b"{bad json\n{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"nope\"}\n");
    let mut output = Vec::new();
    serve_stdio(&mut reader, &mut output, None).expect("stdio serve succeeds");
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["error"]["code"], -32601);
}
