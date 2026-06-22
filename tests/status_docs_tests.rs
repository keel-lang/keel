use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// Only the fields consulted by the remaining docs-sync tests are modelled here;
// serde ignores the other columns present in docs/status/features.json.
#[derive(Debug, Deserialize)]
struct FeatureStatus {
    legend: HashMap<String, StatusLabels>,
    attributes: Vec<AttributeStatus>,
    namespaces: Vec<NamespaceStatus>,
}

#[derive(Debug, Deserialize)]
struct StatusLabels {
    docs: String,
}

#[derive(Debug, Deserialize)]
struct AttributeStatus {
    name: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct NamespaceStatus {
    name: String,
    status: String,
    purpose: String,
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(project_root().join(path)).unwrap_or_else(|err| panic!("read {path}: {err}"))
}

fn feature_status() -> FeatureStatus {
    serde_json::from_str(&read("docs/status/features.json"))
        .expect("parse docs/status/features.json")
}

fn docs_status<'a>(status: &'a FeatureStatus, key: &str) -> &'a str {
    status
        .legend
        .get(key)
        .unwrap_or_else(|| panic!("unknown status key `{key}`"))
        .docs
        .as_str()
}

#[test]
fn stdlib_module_status_table_matches_feature_status_source() {
    let source = feature_status();
    let prelude = read("docs/src/guide/stdlib.md");

    for ns in &source.namespaces {
        let expected = format!(
            "| `{}` | {} | {} |",
            ns.name,
            docs_status(&source, &ns.status),
            ns.purpose
        );
        assert!(
            prelude.contains(&expected),
            "docs/src/guide/stdlib.md module status row drifted from docs/status/features.json:\nexpected row:\n{expected}"
        );
    }
}

#[test]
fn agents_status_claims_match_feature_status_source() {
    let source = feature_status();
    let agents = read("docs/src/guide/agents.md");
    let status_by_name: HashMap<&str, &str> = source
        .attributes
        .iter()
        .map(|attr| (attr.name.as_str(), attr.status.as_str()))
        .collect();

    assert_eq!(status_by_name["@tools [...]"], "shipped");
    assert!(
        agents.contains("capability gating is enforced"),
        "`@tools` is shipped in docs/status/features.json, but docs/src/guide/agents.md no longer says gating is enforced"
    );

    assert_eq!(status_by_name["@team [...]"], "shipped");
    assert!(
        agents.contains("`@team` is used by `broadcast` routing"),
        "`@team` is shipped in docs/status/features.json, but docs/src/guide/agents.md no longer says broadcast routing uses it"
    );

    assert_eq!(status_by_name["@provider <name>"], "shipped");
    assert!(
        agents.contains("`@provider` selects a built-in LLM backend"),
        "`@provider` is shipped in docs/status/features.json, but docs/src/guide/agents.md no longer says it selects a backend"
    );
}
