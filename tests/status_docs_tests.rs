use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct FeatureStatus {
    legend: HashMap<String, StatusLabels>,
    attributes: Vec<AttributeStatus>,
    namespaces: Vec<NamespaceStatus>,
    cli: Vec<CliStatus>,
}

#[derive(Debug, Deserialize)]
struct StatusLabels {
    roadmap: String,
    docs: String,
}

#[derive(Debug, Deserialize)]
struct AttributeStatus {
    name: String,
    tier: String,
    status: String,
    notes: String,
}

#[derive(Debug, Deserialize)]
struct NamespaceStatus {
    name: String,
    status: String,
    implemented_ops: String,
    gaps: String,
    purpose: String,
}

#[derive(Debug, Deserialize)]
struct CliStatus {
    name: String,
    status: String,
    notes: String,
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

fn roadmap_status<'a>(status: &'a FeatureStatus, key: &str) -> &'a str {
    status
        .legend
        .get(key)
        .unwrap_or_else(|| panic!("unknown status key `{key}`"))
        .roadmap
        .as_str()
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
fn roadmap_attribute_status_table_matches_feature_status_source() {
    let source = feature_status();
    let roadmap = read("ROADMAP.md");

    for attr in &source.attributes {
        let expected = format!(
            "| `{}` | {} | {} | {} |",
            attr.name,
            attr.tier,
            roadmap_status(&source, &attr.status),
            attr.notes
        );
        assert!(
            roadmap.contains(&expected),
            "ROADMAP.md attribute status row drifted from docs/status/features.json:\nexpected row:\n{expected}"
        );
    }
}

#[test]
fn roadmap_namespace_status_table_matches_feature_status_source() {
    let source = feature_status();
    let roadmap = read("ROADMAP.md");

    for ns in &source.namespaces {
        let expected = format!(
            "| `{}` | {} | {} | {} |",
            ns.name,
            roadmap_status(&source, &ns.status),
            ns.implemented_ops,
            ns.gaps
        );
        assert!(
            roadmap.contains(&expected),
            "ROADMAP.md namespace status row drifted from docs/status/features.json:\nexpected row:\n{expected}"
        );
    }
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

    assert_eq!(status_by_name["@provider MyProvider"], "planned");
    assert!(
        agents.contains("`@provider` is parsed but has no runtime effect yet"),
        "`@provider` is planned in docs/status/features.json, but docs/src/guide/agents.md no longer says it is unwired"
    );
}

#[test]
fn roadmap_cli_status_table_matches_feature_status_source() {
    let source = feature_status();
    let roadmap = read("ROADMAP.md");

    for cmd in &source.cli {
        let expected = format!(
            "| `{}` | {} | {} |",
            cmd.name,
            roadmap_status(&source, &cmd.status),
            cmd.notes
        );
        assert!(
            roadmap.contains(&expected),
            "ROADMAP.md CLI status row drifted from docs/status/features.json:\nexpected row:\n{expected}"
        );
    }
}
