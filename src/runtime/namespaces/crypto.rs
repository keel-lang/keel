use sha2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};

use crate::interpreter::value::Value;
use crate::interpreter::{CallArgValue, Namespace};
use crate::runtime::args::{expect_int, expect_str, expect_str_named};
use crate::runtime::namespace::{find_arg, ns, positional};

const DEFAULT_TOKEN_BYTES: usize = 32;
// Keep accidental large allocations bounded while leaving plenty of room for
// normal tokens, nonces, salts, and test fixtures.
const MAX_RANDOM_BYTES: usize = 1024 * 1024;

pub(crate) fn namespace() -> Namespace {
    ns!("Crypto", {
        "sha224" => |_interp, args| Box::pin(async move {
            hash_call(args, HashAlgo::Sha224, "Crypto.sha224")
        }),
        "sha256" => |_interp, args| Box::pin(async move {
            hash_call(args, HashAlgo::Sha256, "Crypto.sha256")
        }),
        "sha384" => |_interp, args| Box::pin(async move {
            hash_call(args, HashAlgo::Sha384, "Crypto.sha384")
        }),
        "sha512" => |_interp, args| Box::pin(async move {
            hash_call(args, HashAlgo::Sha512, "Crypto.sha512")
        }),
        "sha512_224" => |_interp, args| Box::pin(async move {
            hash_call(args, HashAlgo::Sha512_224, "Crypto.sha512_224")
        }),
        "sha512_256" => |_interp, args| Box::pin(async move {
            hash_call(args, HashAlgo::Sha512_256, "Crypto.sha512_256")
        }),
        "hmac_sha224" => |_interp, args| Box::pin(async move {
            hmac_call(args, HashAlgo::Sha224, "Crypto.hmac_sha224")
        }),
        "hmac_sha256" => |_interp, args| Box::pin(async move {
            hmac_call(args, HashAlgo::Sha256, "Crypto.hmac_sha256")
        }),
        "hmac_sha384" => |_interp, args| Box::pin(async move {
            hmac_call(args, HashAlgo::Sha384, "Crypto.hmac_sha384")
        }),
        "hmac_sha512" => |_interp, args| Box::pin(async move {
            hmac_call(args, HashAlgo::Sha512, "Crypto.hmac_sha512")
        }),
        "hmac_sha512_224" => |_interp, args| Box::pin(async move {
            hmac_call(args, HashAlgo::Sha512_224, "Crypto.hmac_sha512_224")
        }),
        "hmac_sha512_256" => |_interp, args| Box::pin(async move {
            hmac_call(args, HashAlgo::Sha512_256, "Crypto.hmac_sha512_256")
        }),
        "token" => |_interp, args| Box::pin(async move {
            let n = byte_count_arg(&args, DEFAULT_TOKEN_BYTES, "Crypto.token")?;
            let bytes = secure_random_bytes(n, "Crypto.token")?;
            Ok(Value::String(hex(&bytes)))
        }),
        "random_bytes" => |_interp, args| Box::pin(async move {
            let n = expect_int(&args, 0, "Crypto.random_bytes")?;
            let n = validate_byte_count(n, "Crypto.random_bytes")?;
            let bytes = secure_random_bytes(n, "Crypto.random_bytes")?;
            Ok(Value::List(
                bytes
                    .into_iter()
                    .map(|b| Value::Integer(i64::from(b)))
                    .collect(),
            ))
        }),
    })
}

#[derive(Clone, Copy)]
enum HashAlgo {
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha512_224,
    Sha512_256,
}

fn hash_call(args: Vec<CallArgValue>, algo: HashAlgo, context: &str) -> miette::Result<Value> {
    let data = expect_str(&args, 0, context)?;
    Ok(Value::String(hash_hex(data.as_bytes(), algo)))
}

fn hmac_call(args: Vec<CallArgValue>, algo: HashAlgo, context: &str) -> miette::Result<Value> {
    let data = expect_str(&args, 0, context)?;
    let key = expect_str_named(&args, "key", context)?
        .ok_or_else(|| miette::miette!("{context}: missing `key:` argument"))?;
    Ok(Value::String(hmac_hex(
        key.as_bytes(),
        data.as_bytes(),
        algo,
    )))
}

fn hash_hex(data: &[u8], algo: HashAlgo) -> String {
    match algo {
        HashAlgo::Sha224 => hex(&Sha224::digest(data)),
        HashAlgo::Sha256 => hex(&Sha256::digest(data)),
        HashAlgo::Sha384 => hex(&Sha384::digest(data)),
        HashAlgo::Sha512 => hex(&Sha512::digest(data)),
        HashAlgo::Sha512_224 => hex(&Sha512_224::digest(data)),
        HashAlgo::Sha512_256 => hex(&Sha512_256::digest(data)),
    }
}

fn hmac_hex(key: &[u8], data: &[u8], algo: HashAlgo) -> String {
    match algo {
        HashAlgo::Sha224 => hex(&hmac::<Sha224>(key, data, 64)),
        HashAlgo::Sha256 => hex(&hmac::<Sha256>(key, data, 64)),
        HashAlgo::Sha384 => hex(&hmac::<Sha384>(key, data, 128)),
        HashAlgo::Sha512 => hex(&hmac::<Sha512>(key, data, 128)),
        HashAlgo::Sha512_224 => hex(&hmac::<Sha512_224>(key, data, 128)),
        HashAlgo::Sha512_256 => hex(&hmac::<Sha512_256>(key, data, 128)),
    }
}

fn hmac<D>(key: &[u8], data: &[u8], block_size: usize) -> Vec<u8>
where
    D: Digest + Default,
{
    let mut key_block = vec![0_u8; block_size];
    if key.len() > block_size {
        let hashed = D::digest(key);
        key_block[..hashed.len()].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut outer = vec![0x5c_u8; block_size];
    let mut inner = vec![0x36_u8; block_size];
    for (idx, key_byte) in key_block.iter().enumerate() {
        outer[idx] ^= key_byte;
        inner[idx] ^= key_byte;
    }

    let mut inner_hasher = D::new();
    inner_hasher.update(&inner);
    inner_hasher.update(data);
    let inner_digest = inner_hasher.finalize();

    let mut outer_hasher = D::new();
    outer_hasher.update(&outer);
    outer_hasher.update(inner_digest);
    outer_hasher.finalize().to_vec()
}

fn byte_count_arg(
    args: &[crate::interpreter::CallArgValue],
    default: usize,
    context: &str,
) -> miette::Result<usize> {
    let Some(value) = find_arg(args, "bytes").or_else(|| positional(args, 0)) else {
        return Ok(default);
    };
    let n = value
        .as_int()
        .ok_or_else(|| miette::miette!("{context}: `bytes:` must be an integer"))?;
    validate_byte_count(n, context)
}

fn validate_byte_count(n: i64, context: &str) -> miette::Result<usize> {
    if n < 0 {
        return Err(miette::miette!(
            "{context}: byte count must be non-negative"
        ));
    }
    let n =
        usize::try_from(n).map_err(|_| miette::miette!("{context}: byte count is too large"))?;
    if n > MAX_RANDOM_BYTES {
        return Err(miette::miette!(
            "{context}: byte count must be <= {MAX_RANDOM_BYTES}"
        ));
    }
    Ok(n)
}

fn secure_random_bytes(n: usize, context: &str) -> miette::Result<Vec<u8>> {
    let mut bytes = vec![0_u8; n];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| miette::miette!("{context}: random source failed: {e}"))?;
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing hex to String cannot fail");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::interpreter::{CallArgValue, Interpreter};

    fn arg(value: Value) -> CallArgValue {
        CallArgValue { name: None, value }
    }

    fn named(name: &str, value: Value) -> CallArgValue {
        CallArgValue {
            name: Some(name.to_string()),
            value,
        }
    }

    #[test]
    fn namespace_has_crypto_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "Crypto");
        for method in [
            "sha224",
            "sha256",
            "sha384",
            "sha512",
            "sha512_224",
            "sha512_256",
            "hmac_sha224",
            "hmac_sha256",
            "hmac_sha384",
            "hmac_sha512",
            "hmac_sha512_224",
            "hmac_sha512_256",
        ] {
            assert!(ns.methods.contains_key(method), "missing {method}");
        }
        assert!(!ns.methods.contains_key("hash"));
        assert!(!ns.methods.contains_key("hmac"));
        assert!(ns.methods.contains_key("token"));
        assert!(ns.methods.contains_key("random_bytes"));
    }

    #[test]
    fn hash_matches_sha256_vector() {
        let got = hash_hex(b"hello", HashAlgo::Sha256);
        assert_eq!(
            got,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn hash_supports_safe_sha2_variants() {
        let cases = [
            (HashAlgo::Sha224, 56),
            (HashAlgo::Sha256, 64),
            (HashAlgo::Sha384, 96),
            (HashAlgo::Sha512, 128),
            (HashAlgo::Sha512_224, 56),
            (HashAlgo::Sha512_256, 64),
        ];

        for (algo, expected_len) in cases {
            let got = hash_hex(b"hello", algo);
            assert_eq!(got.len(), expected_len);
            assert!(got.chars().all(|ch| ch.is_ascii_hexdigit()));
        }

        let sha384 = hash_hex(b"hello", HashAlgo::Sha384);
        assert_eq!(
            sha384,
            "59e1748777448c69de6b800d7a33bbfb9ff1b463e44354c3553bcdb9c666fa90125a3c79f90397bdf5f6a13de828684f"
        );
    }

    #[test]
    fn hmac_matches_sha256_vector() {
        let got = hmac_hex(
            b"key",
            b"The quick brown fox jumps over the lazy dog",
            HashAlgo::Sha256,
        );
        assert_eq!(
            got,
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn hmac_supports_safe_sha2_variants() {
        let cases = [
            (HashAlgo::Sha224, 56),
            (HashAlgo::Sha256, 64),
            (HashAlgo::Sha384, 96),
            (HashAlgo::Sha512, 128),
            (HashAlgo::Sha512_224, 56),
            (HashAlgo::Sha512_256, 64),
        ];

        for (algo, expected_len) in cases {
            let got = hmac_hex(b"key", b"hello", algo);
            assert_eq!(got.len(), expected_len);
            assert!(got.chars().all(|ch| ch.is_ascii_hexdigit()));
        }
    }

    #[tokio::test]
    async fn token_returns_hex_for_requested_byte_count() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("token").expect("token method");
        let value = method(&mut interp, vec![named("bytes", Value::Integer(16))])
            .await
            .expect("token");
        let Value::String(token) = value else {
            panic!("expected token string");
        };
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn random_bytes_returns_int_list() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("random_bytes").expect("random_bytes method");
        let value = method(&mut interp, vec![arg(Value::Integer(8))])
            .await
            .expect("random bytes");
        let Value::List(items) = value else {
            panic!("expected byte list");
        };
        assert_eq!(items.len(), 8);
        assert!(
            items
                .into_iter()
                .all(|item| { matches!(item, Value::Integer(n) if (0..=255).contains(&n)) })
        );
    }

    #[tokio::test]
    async fn random_bytes_rejects_non_integer_count() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("random_bytes").expect("random_bytes method");
        let err = method(&mut interp, vec![arg(Value::String("8".into()))])
            .await
            .expect_err("string byte count must fail");
        assert_eq!(
            err.to_string(),
            "Crypto.random_bytes: argument at position 0 must be int, got str"
        );
    }

    #[tokio::test]
    async fn sha256_method_matches_vector() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("sha256").expect("sha256 method");
        let value = method(&mut interp, vec![arg(Value::String("hello".into()))])
            .await
            .expect("sha256");
        assert_eq!(
            value,
            Value::String(
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into()
            )
        );
    }
}
