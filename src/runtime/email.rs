use std::collections::HashMap;

use colored::Colorize;

use crate::interpreter::value::Value;

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
                "host" => imap_host = val.as_string(),
                "smtp_host" => smtp_host = val.as_string(),
                "user" => user = val.as_string(),
                "pass" | "password" => pass = val.as_string(),
                _ => {}
            }
        }

        // Default SMTP host from IMAP host (common pattern)
        if smtp_host.is_empty() && !imap_host.is_empty() {
            smtp_host = imap_host.replace("imap.", "smtp.");
        }

        if imap_host.is_empty() || user.is_empty() || pass.is_empty() {
            return Err(
                "Email connection requires host, user, and pass fields".to_string()
            );
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

    let client = imap::connect(
        (&conn.imap_host as &str, 993),
        &conn.imap_host,
        &tls,
    )
    .map_err(|e| format!("IMAP connect failed ({}): {}", conn.imap_host, e))?;

    let mut session = client
        .login(&conn.user, &conn.pass)
        .map_err(|e| format!("IMAP login failed: {}", e.0))?;

    session.select("INBOX").map_err(|e| format!("IMAP select INBOX: {e}"))?;

    let unseen = session
        .search("UNSEEN")
        .map_err(|e| format!("IMAP search: {e}"))?;

    let mut emails = Vec::new();

    if unseen.is_empty() {
        session.logout().ok();
        return Ok(emails);
    }

    // Fetch up to 20 most recent unseen
    let mut id_list: Vec<u32> = unseen.iter().cloned().collect();
    id_list.sort_unstable();
    id_list.reverse();
    id_list.truncate(20);
    let fetch_range = id_list.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");

    let messages = session
        .fetch(&fetch_range, "RFC822")
        .map_err(|e| format!("IMAP fetch: {e}"))?;

    for msg in messages.iter() {
        if let Some(body) = msg.body() {
            let parsed = parse_email(body);
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

/// Send an email via SMTP.
pub fn send_email(
    conn: &EmailConnection,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    use lettre::{Message, SmtpTransport, Transport};
    use lettre::transport::smtp::authentication::Credentials;

    let email = Message::builder()
        .from(conn.user.parse().map_err(|e| format!("Invalid from address: {e}"))?)
        .to(to.parse().map_err(|e| format!("Invalid to address: {e}"))?)
        .subject(subject)
        .body(body.to_string())
        .map_err(|e| format!("Failed to build email: {e}"))?;

    let creds = Credentials::new(conn.user.clone(), conn.pass.clone());

    let mailer = SmtpTransport::relay(&conn.smtp_host)
        .map_err(|e| format!("SMTP relay error: {e}"))?
        .credentials(creds)
        .build();

    mailer.send(&email).map_err(|e| format!("SMTP send failed: {e}"))?;

    println!(
        "  {} Email sent to {}",
        "✓".bright_green(),
        to.bright_cyan()
    );

    Ok(())
}

/// Parse a raw email into a Value::Map.
fn parse_email(raw: &[u8]) -> Value {
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
    map.insert("from".to_string(), Value::String(from));
    map.insert("subject".to_string(), Value::String(subject));
    map.insert("body".to_string(), Value::String(body));
    map.insert("unread".to_string(), Value::Bool(true));
    Value::Map(map)
}
