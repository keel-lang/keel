use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{find_arg, ns, positional};

const UUID_DNS: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const UUID_URL: &str = "6ba7b811-9dad-11d1-80b4-00c04fd430c8";
const UUID_OID: &str = "6ba7b812-9dad-11d1-80b4-00c04fd430c8";
const UUID_X500: &str = "6ba7b814-9dad-11d1-80b4-00c04fd430c8";

pub(crate) fn namespace() -> Namespace {
    ns!("Uuid", {
        "v4" => |_interp, _args| Box::pin(async move {
            let mut bytes = random_bytes()?;
            set_version_and_variant(&mut bytes, 4);
            Ok(Value::Uuid(format_uuid(bytes)))
        }),
        "v7" => |interp, _args| Box::pin(async move {
            let mut bytes = random_bytes()?;
            let ts = interp.runtime.clock.now_utc().timestamp_millis().max(0) as u64;
            bytes[0] = ((ts >> 40) & 0xff) as u8;
            bytes[1] = ((ts >> 32) & 0xff) as u8;
            bytes[2] = ((ts >> 24) & 0xff) as u8;
            bytes[3] = ((ts >> 16) & 0xff) as u8;
            bytes[4] = ((ts >> 8) & 0xff) as u8;
            bytes[5] = (ts & 0xff) as u8;
            set_version_and_variant(&mut bytes, 7);
            Ok(Value::Uuid(format_uuid(bytes)))
        }),
        "v5" => |_interp, args| Box::pin(async move {
            let ns = find_arg(&args, "ns")
                .or_else(|| positional(&args, 0))
                .ok_or_else(|| miette::miette!("Uuid.v5: missing `ns:` argument"))?;
            let name = find_arg(&args, "name")
                .or_else(|| positional(&args, 1))
                .map(Value::to_display_string)
                .ok_or_else(|| miette::miette!("Uuid.v5: missing `name:` argument"))?;
            let ns_bytes = uuid_bytes_from_value(ns)
                .ok_or_else(|| miette::miette!("Uuid.v5: `ns:` must be a Uuid value"))?;
            let digest = sha1_uuid(&ns_bytes, name.as_bytes());
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(&digest[..16]);
            set_version_and_variant(&mut bytes, 5);
            Ok(Value::Uuid(format_uuid(bytes)))
        }),
        "parse" => |_interp, args| Box::pin(async move {
            let s = positional(&args, 0)
                .map(Value::to_display_string)
                .ok_or_else(|| miette::miette!("Uuid.parse: missing argument"))?;
            Ok(parse_uuid(&s)
                .map(|bytes| Value::Uuid(format_uuid(bytes)))
                .unwrap_or(Value::None))
        }),
    })
}

pub(crate) fn uuid_namespace_constant(field: &str) -> Option<Value> {
    match field {
        "DNS" => Some(Value::Uuid(UUID_DNS.to_string())),
        "URL" => Some(Value::Uuid(UUID_URL.to_string())),
        "OID" => Some(Value::Uuid(UUID_OID.to_string())),
        "X500" => Some(Value::Uuid(UUID_X500.to_string())),
        _ => None,
    }
}

fn random_bytes() -> miette::Result<[u8; 16]> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| miette::miette!("Uuid: random source failed: {e}"))?;
    Ok(bytes)
}

fn uuid_bytes_from_value(value: &Value) -> Option<[u8; 16]> {
    match value {
        Value::Uuid(id) => parse_uuid(id),
        Value::EnumVariant(ty, name, _) if ty == "Uuid" => {
            uuid_namespace_constant(name).and_then(|v| match v {
                Value::Uuid(id) => parse_uuid(&id),
                _ => None,
            })
        }
        _ => None,
    }
}

fn set_version_and_variant(bytes: &mut [u8; 16], version: u8) {
    bytes[6] = (bytes[6] & 0x0f) | (version << 4);
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
}

fn parse_uuid(s: &str) -> Option<[u8; 16]> {
    let raw = s.strip_prefix("urn:uuid:").unwrap_or(s);
    let mut hex = String::with_capacity(32);
    for ch in raw.chars() {
        if ch == '-' {
            continue;
        }
        if !ch.is_ascii_hexdigit() {
            return None;
        }
        hex.push(ch.to_ascii_lowercase());
    }
    if hex.len() != 32 {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        let start = i * 2;
        *b = u8::from_str_radix(&hex[start..start + 2], 16).ok()?;
    }
    Some(bytes)
}

fn format_uuid(bytes: [u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn sha1_uuid(namespace: &[u8; 16], name: &[u8]) -> [u8; 20] {
    let mut sha = Sha1::new();
    sha.update(namespace);
    sha.update(name);
    sha.finalize()
}

struct Sha1 {
    state: [u32; 5],
    len: u64,
    buffer: Vec<u8>,
}

impl Sha1 {
    fn new() -> Self {
        Self {
            state: [
                0x6745_2301,
                0xefcd_ab89,
                0x98ba_dcfe,
                0x1032_5476,
                0xc3d2_e1f0,
            ],
            len: 0,
            buffer: Vec::with_capacity(64),
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.len += data.len() as u64;
        self.buffer.extend_from_slice(data);
        while self.buffer.len() >= 64 {
            let mut block = [0_u8; 64];
            block.copy_from_slice(&self.buffer[..64]);
            self.process_block(&block);
            self.buffer.drain(..64);
        }
    }

    fn finalize(mut self) -> [u8; 20] {
        let bit_len = self.len * 8;
        self.buffer.push(0x80);
        while self.buffer.len() % 64 != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());
        while self.buffer.len() >= 64 {
            let mut block = [0_u8; 64];
            block.copy_from_slice(&self.buffer[..64]);
            self.process_block(&block);
            self.buffer.drain(..64);
        }
        let mut out = [0_u8; 20];
        for (chunk, word) in out.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        let mut w = [0_u32; 80];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = self.state;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
    }
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
    fn namespace_has_uuid_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "Uuid");
        assert!(ns.methods.contains_key("v4"));
        assert!(ns.methods.contains_key("v7"));
        assert!(ns.methods.contains_key("v5"));
        assert!(ns.methods.contains_key("parse"));
    }

    #[test]
    fn parse_accepts_hyphenated_simple_and_urn() {
        let bytes = parse_uuid("f47ac10b-58cc-4372-a567-0e02b2c3d479").expect("hyphenated");
        assert_eq!(format_uuid(bytes), "f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert!(parse_uuid("f47ac10b58cc4372a5670e02b2c3d479").is_some());
        assert!(parse_uuid("urn:uuid:f47ac10b-58cc-4372-a567-0e02b2c3d479").is_some());
        assert!(parse_uuid("not-a-uuid").is_none());
    }

    #[test]
    fn sha1_matches_known_empty_digest() {
        let digest = Sha1::new().finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
    }

    #[tokio::test]
    async fn v4_returns_version_4_uuid() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("v4").expect("v4 method");
        let value = method(&mut interp, vec![]).await.expect("uuid v4");
        let Value::Uuid(id) = value else {
            panic!("expected Uuid");
        };
        assert_eq!(id.as_bytes()[14], b'4');
    }

    #[tokio::test]
    async fn v5_matches_dns_example() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("v5").expect("v5 method");
        let value = method(
            &mut interp,
            vec![
                named("ns", Value::Uuid(UUID_DNS.to_string())),
                named("name", Value::String("www.example.com".to_string())),
            ],
        )
        .await
        .expect("uuid v5");
        assert_eq!(
            value,
            Value::Uuid("2ed6657d-e927-568b-95e1-2665a8aea6a2".to_string())
        );
    }

    #[tokio::test]
    async fn parse_returns_none_for_invalid_input() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("parse").expect("parse method");
        let value = method(&mut interp, vec![arg(Value::String("bad".into()))])
            .await
            .expect("parse result");
        assert_eq!(value, Value::None);
    }
}
