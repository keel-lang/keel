use std::collections::HashMap;

use colored::Colorize;

use crate::interpreter::value::{MapKey, Value};

/// Email connection configuration.
#[derive(Debug, Clone)]
pub struct EmailConnection {
    pub imap_host: String,
    pub smtp_host: String,
    pub user: String,
    pub pass: String,
}

impl EmailConnection {
    pub fn from_config(config: &[(String, Value)]) -> Result<Self, String> {
        let mut imap_host = String::new();
        let mut smtp_host = String::new();
        let mut user = String::new();
        let mut pass = String::new();

        for (key, val) in config {
            match key.as_str() {
                "host" => imap_host = val.to_display_string(),
                "smtp_host" => smtp_host = val.to_display_string(),
                "user" => user = val.to_display_string(),
                "pass" | "password" => pass = val.to_display_string(),
                _ => {}
            }
        }

        // Default SMTP host from IMAP host (common pattern)
        if smtp_host.is_empty() && !imap_host.is_empty() {
            smtp_host = imap_host.replace("imap.", "smtp.");
        }

        if imap_host.is_empty() || user.is_empty() || pass.is_empty() {
            return Err("Email connection requires host, user, and pass fields".to_string());
        }

        Ok(EmailConnection {
            imap_host,
            smtp_host,
            user,
            pass,
        })
    }
}

/// Fetch unread emails via IMAP. Returns a list of email maps.
pub fn fetch_emails(conn: &EmailConnection) -> Result<Vec<Value>, String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS error: {e}"))?;

    let client = imap::connect((&conn.imap_host as &str, 993), &conn.imap_host, &tls)
        .map_err(|e| format!("IMAP connect failed ({}): {}", conn.imap_host, e))?;

    let mut session = client
        .login(&conn.user, &conn.pass)
        .map_err(|e| format!("IMAP login failed: {}", e.0))?;

    session
        .select("INBOX")
        .map_err(|e| format!("IMAP select INBOX: {e}"))?;

    let unseen = session
        .uid_search("UNSEEN")
        .map_err(|e| format!("IMAP search: {e}"))?;

    let mut emails = Vec::new();

    if unseen.is_empty() {
        session.logout().ok();
        return Ok(emails);
    }

    // Fetch up to 20 most recent unseen, by UID.
    let mut uid_list: Vec<u32> = unseen.iter().cloned().collect();
    uid_list.sort_unstable();
    uid_list.reverse();
    uid_list.truncate(20);
    let fetch_range = uid_list
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let messages = session
        .uid_fetch(&fetch_range, "(UID RFC822)")
        .map_err(|e| format!("IMAP fetch: {e}"))?;

    for msg in messages.iter() {
        if let Some(body) = msg.body() {
            let parsed = parse_email(body, msg.uid);
            emails.push(parsed);
        }
    }

    session.logout().ok();

    println!(
        "  {} Fetched {} email(s) via IMAP",
        "✓".bright_green(),
        emails.len()
    );

    Ok(emails)
}

/// Move an email by UID into the configured archive folder. The folder
/// is read from `IMAP_ARCHIVE_FOLDER` (default `Archive`). If the server
/// supports the IMAP MOVE extension we use it; otherwise we fall back to
/// COPY + STORE (\\Deleted) + EXPUNGE.
pub fn archive_email(conn: &EmailConnection, uid: u32, folder: &str) -> Result<(), String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS error: {e}"))?;

    let client = imap::connect((&conn.imap_host as &str, 993), &conn.imap_host, &tls)
        .map_err(|e| format!("IMAP connect failed ({}): {}", conn.imap_host, e))?;

    let mut session = client
        .login(&conn.user, &conn.pass)
        .map_err(|e| format!("IMAP login failed: {}", e.0))?;

    session
        .select("INBOX")
        .map_err(|e| format!("IMAP select INBOX: {e}"))?;

    let uid_str = uid.to_string();
    if session.uid_mv(&uid_str, folder).is_err() {
        // Fallback for servers without MOVE support.
        session
            .uid_copy(&uid_str, folder)
            .map_err(|e| format!("IMAP UID COPY to `{folder}`: {e}"))?;
        session
            .uid_store(&uid_str, "+FLAGS (\\Deleted)")
            .map_err(|e| format!("IMAP UID STORE: {e}"))?;
        session
            .expunge()
            .map_err(|e| format!("IMAP EXPUNGE: {e}"))?;
    }

    session.logout().ok();

    println!(
        "  {} Archived email UID {} → {}",
        "✓".bright_green(),
        uid,
        folder.bright_cyan()
    );

    Ok(())
}

/// Send an email via SMTP.
pub fn send_email(
    conn: &EmailConnection,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};

    let email = Message::builder()
        .from(
            conn.user
                .parse()
                .map_err(|e| format!("Invalid from address: {e}"))?,
        )
        .to(to.parse().map_err(|e| format!("Invalid to address: {e}"))?)
        .subject(subject)
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {e}"))?;

    let creds = Credentials::new(conn.user.clone(), conn.pass.clone());

    let mailer = SmtpTransport::relay(&conn.smtp_host)
        .map_err(|e| format!("SMTP relay error: {e}"))?
        .credentials(creds)
        .build();

    mailer
        .send(&email)
        .map_err(|e| format!("SMTP send failed: {e}"))?;

    println!(
        "  {} Email sent to {}",
        "✓".bright_green(),
        to.bright_cyan()
    );

    Ok(())
}

/// Parse a raw email into a Value::Map. The IMAP UID is stored under
/// `uid` so `Email.archive(message)` can move the message later.
fn parse_email(raw: &[u8], uid: Option<u32>) -> Value {
    let text = String::from_utf8_lossy(raw);
    let mut from = String::new();
    let mut subject = String::new();
    let mut body = String::new();
    let mut in_headers = true;

    for line in text.lines() {
        if in_headers {
            if line.is_empty() {
                in_headers = false;
                continue;
            }
            if let Some(val) = line.strip_prefix("From: ") {
                from = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("Subject: ") {
                subject = val.trim().to_string();
            }
        } else {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
    }

    let mut map = HashMap::new();
    map.insert(MapKey::Str("from".into()), Value::String(from));
    map.insert(MapKey::Str("subject".into()), Value::String(subject));
    map.insert(MapKey::Str("body".into()), Value::String(body));
    map.insert(MapKey::Str("unread".into()), Value::Bool(true));
    if let Some(u) = uid {
        map.insert(MapKey::Str("uid".into()), Value::Integer(u as i64));
    }
    Value::Map(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
        match value {
            Value::Map(map) => map
                .get(&MapKey::Str(key.to_string()))
                .expect("field exists"),
            other => panic!("expected email map, got {}", other.type_name()),
        }
    }

    #[test]
    fn connection_from_config_requires_host_user_and_password() {
        let config = vec![
            (
                "host".to_string(),
                Value::String("imap.example.test".into()),
            ),
            ("user".to_string(), Value::String("bot@example.test".into())),
        ];

        let err = EmailConnection::from_config(&config).expect_err("missing password");
        assert_eq!(err, "Email connection requires host, user, and pass fields");
    }

    #[test]
    fn connection_from_config_derives_smtp_host() {
        let config = vec![
            (
                "host".to_string(),
                Value::String("imap.example.test".into()),
            ),
            ("user".to_string(), Value::String("bot@example.test".into())),
            ("pass".to_string(), Value::String("secret".into())),
        ];

        let conn = EmailConnection::from_config(&config).expect("connection config");
        assert_eq!(conn.imap_host, "imap.example.test");
        assert_eq!(conn.smtp_host, "smtp.example.test");
        assert_eq!(conn.user, "bot@example.test");
        assert_eq!(conn.pass, "secret");
    }

    #[test]
    fn connection_from_config_accepts_explicit_smtp_host_and_password_alias() {
        let config = vec![
            (
                "host".to_string(),
                Value::String("mail.example.test".into()),
            ),
            (
                "smtp_host".to_string(),
                Value::String("smtp-relay.example.test".into()),
            ),
            ("user".to_string(), Value::String("bot@example.test".into())),
            ("password".to_string(), Value::String("secret".into())),
        ];

        let conn = EmailConnection::from_config(&config).expect("connection config");
        assert_eq!(conn.smtp_host, "smtp-relay.example.test");
        assert_eq!(conn.pass, "secret");
    }

    #[test]
    fn parse_email_extracts_headers_body_and_uid() {
        let raw = b"From: Ada <ada@example.test>\r\nSubject: Notes\r\nDate: today\r\n\r\nLine one\r\nLine two";
        let parsed = parse_email(raw, Some(42));

        assert_eq!(
            field(&parsed, "from"),
            &Value::String("Ada <ada@example.test>".into())
        );
        assert_eq!(field(&parsed, "subject"), &Value::String("Notes".into()));
        assert_eq!(
            field(&parsed, "body"),
            &Value::String("Line one\nLine two".into())
        );
        assert_eq!(field(&parsed, "unread"), &Value::Bool(true));
        assert_eq!(field(&parsed, "uid"), &Value::Integer(42));
    }

    #[test]
    fn parse_email_without_uid_omits_uid_field() {
        let parsed = parse_email(b"From: a@example.test\nSubject: Hi\n\nBody", None);
        let Value::Map(map) = parsed else {
            panic!("expected email map");
        };

        assert!(!map.contains_key(&MapKey::Str("uid".into())));
        assert_eq!(
            map.get(&MapKey::Str("body".into())),
            Some(&Value::String("Body".into()))
        );
    }

    #[test]
    fn send_email_rejects_invalid_from_address_before_smtp() {
        let conn = EmailConnection {
            imap_host: "imap.example.test".into(),
            smtp_host: "smtp.example.test".into(),
            user: "not an email".into(),
            pass: "secret".into(),
        };

        let err = send_email(&conn, "ops@example.test", "subject", "body")
            .expect_err("invalid from address");
        assert!(err.contains("Invalid from address"), "{err}");
    }

    #[test]
    fn send_email_rejects_invalid_to_address_before_smtp() {
        let conn = EmailConnection {
            imap_host: "imap.example.test".into(),
            smtp_host: "smtp.example.test".into(),
            user: "bot@example.test".into(),
            pass: "secret".into(),
        };

        let err =
            send_email(&conn, "not an email", "subject", "body").expect_err("invalid to address");
        assert!(err.contains("Invalid to address"), "{err}");
    }
}
