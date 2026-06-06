use crate::common::*;

// ---------------------------------------------------------------------------
// Json namespace
// ---------------------------------------------------------------------------

#[test]
fn json_namespace_stringifies_and_parses_maps() {
    let src = r#"
agent A {
    @on_start {
        data = {name: "Ada", age: 42, tags: ["ai", "lang"], active: true}
        text = Json.stringify(data)
        parsed = Json.parse(text)
        Io.show(text)
        Io.show("{parsed.name}:{parsed.age}:{parsed.tags.join("|")}:{parsed.active}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Json namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("\"name\":\"Ada\""), "{stdout}");
    assert!(stdout.contains("Ada:42:ai|lang:true"), "{stdout}");
}

#[test]
fn json_namespace_invalid_parse_reports_json_error() {
    let src = r#"
agent A {
    @on_start {
        Json.parse("not json")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "invalid Json.parse should fail");
    assert!(
        stderr.contains("JsonError: Json.parse: invalid JSON"),
        "expected JsonError diagnostic:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Random namespace
// ---------------------------------------------------------------------------

#[test]
fn random_namespace_generates_values_in_expected_shapes() {
    let src = r#"
agent A {
    @on_start {
        roll = Random.int(min: 1, max: 6)
        sample = Random.float()
        enabled = Random.bool()
        if roll >= 1 and roll <= 6 and sample >= 0.0 and sample < 1.0 {
            Io.show("random-ok {enabled}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Random namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("random-ok"), "{stdout}");
}

#[test]
fn random_namespace_rejects_inverted_int_bounds() {
    let src = r#"
agent A {
    @on_start {
        Random.int(min: 5, max: 1)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "inverted Random.int bounds should fail");
    assert!(
        stderr.contains("Random.int: `min:` must be <= `max:`"),
        "expected Random.int diagnostic:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Uuid namespace
// ---------------------------------------------------------------------------

#[test]
fn uuid_namespace_generates_parses_and_formats_values() {
    let src = r#"
agent A {
    @on_start {
        id: Uuid = uuid()
        v4 = id.version()
        v7 = Uuid.v7().version()
        site = Uuid.v5(ns: Uuid.DNS, name: "www.example.com")
        parsed = Uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479")
        missing = Uuid.parse("bad")
        simple = id.format(as: "simple")
        urn = id.format(as: "urn")
        if v4 == 4 and v7 == 7 and site.to_str() == "2ed6657d-e927-568b-95e1-2665a8aea6a2" and parsed != none and missing == none {
            Io.show("uuid-ok {simple} {urn}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Uuid namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("uuid-ok"), "{stdout}");
    assert!(stdout.contains("urn:uuid:"), "{stdout}");
}

// ---------------------------------------------------------------------------
// Crypto namespace
// ---------------------------------------------------------------------------

#[test]
fn crypto_namespace_hashes_signs_and_generates_random_values() {
    let src = r#"
agent A {
    @on_start {
        digest = Crypto.sha256("hello")
        wide = Crypto.sha384("hello")
        sig = Crypto.hmac_sha256("The quick brown fox jumps over the lazy dog", key: "key")
        token = Crypto.token(bytes: 16)
        bytes = Crypto.random_bytes(4)
        if digest == "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824" and wide == "59e1748777448c69de6b800d7a33bbfb9ff1b463e44354c3553bcdb9c666fa90125a3c79f90397bdf5f6a13de828684f" and sig == "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8" and token.len() == 32 and bytes.len() == 4 {
            Io.show("crypto-ok")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Crypto namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("crypto-ok"), "{stdout}");
}

#[test]
fn crypto_namespace_does_not_expose_generic_hash_selection() {
    let src = r#"
agent A {
    @on_start {
        Crypto.hash("hello", algo: "md5")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "generic Crypto.hash should fail");
    assert!(
        stderr.contains("Namespace `Crypto` has no method `hash`"),
        "expected missing Crypto.hash diagnostic:\n{stderr}"
    );
}
