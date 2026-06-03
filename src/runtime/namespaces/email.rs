use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::value::{MapKey, Value};
use crate::runtime::args::{expect_bool_named, expect_str_value};
use crate::runtime::namespace::{find_arg, ns, positional};
use crate::runtime::{context, email};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "Email",
        name: "fetch",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Fetch messages from the configured email inbox.",
    },
    BuiltinMethod {
        namespace: "Email",
        name: "send",
        params: &[
            BuiltinParam {
                name: "to",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "subject",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Send an email.",
    },
    BuiltinMethod {
        namespace: "Email",
        name: "archive",
        params: &[BuiltinParam {
            name: "id",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Archive an email by its ID.",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("Email", {
        "fetch" => |host, args| Box::pin(async move {
            // `unread: true` is the v0.1 default (and only) filter.
            let _unread_only = expect_bool_named(&args, "unread", "Email.fetch")?.unwrap_or(true);
            let Some(conn) = email_conn_from_env(host.runtime().env.as_ref()) else {
                eprintln!("  ⚠ Email.fetch: IMAP_HOST/EMAIL_USER/EMAIL_PASS not set — returning empty list");
                return Ok(Value::List(vec![]));
            };
            match tokio::task::spawn_blocking(move || email::fetch_emails(&conn)).await {
                Ok(Ok(emails)) => Ok(Value::List(emails)),
                Ok(Err(msg)) => Err(miette::miette!("{msg}")),
                Err(e) => Err(miette::miette!("email fetch task join error: {e}")),
            }
        }),
        "send" => |host, args| Box::pin(async move {
            let Some(conn) = email_conn_from_env(host.runtime().env.as_ref()) else {
                eprintln!("  ⚠ Email.send: IMAP_HOST/EMAIL_USER/EMAIL_PASS not set — skipping");
                return Ok(Value::None);
            };
            // Positional 0 is the message body (str or Map with .body).
            let (body, inferred_subject) = match positional(&args, 0) {
                Some(Value::Map(m)) => (
                    email_text_field(m, "body", "Email.send")?
                        .unwrap_or_default()
                        .to_owned(),
                    email_text_field(m, "subject", "Email.send")?.map(str::to_owned),
                ),
                Some(v) => (
                    expect_str_value(v, "message body", "Email.send")?.to_owned(),
                    None,
                ),
                None => return Err(miette::miette!("Email.send: missing message body")),
            };
            let to = match find_arg(&args, "to") {
                Some(Value::Map(m)) => email_text_field(m, "from", "Email.send")?
                    .unwrap_or_default()
                    .to_owned(),
                Some(v) => expect_str_value(v, "`to:`", "Email.send")?.to_owned(),
                None => return Err(miette::miette!("Email.send: missing `to:` argument")),
            };
            let subject = find_arg(&args, "subject")
                .map(|v| expect_str_value(v, "`subject:`", "Email.send").map(str::to_owned))
                .transpose()?
                .or(inferred_subject)
                .unwrap_or_else(|| "(no subject)".to_string());
            match tokio::task::spawn_blocking(move || email::send_email(&conn, &to, &subject, &body)).await {
                Ok(Ok(())) => Ok(Value::None),
                Ok(Err(msg)) => Err(miette::miette!("{msg}")),
                Err(e) => Err(miette::miette!("email send task join error: {e}")),
            }
        }),
        // Email.archive(message) — move a fetched email out of INBOX
        // into the folder named by IMAP_ARCHIVE_FOLDER (default `Archive`).
        // The message's UID is read from message.uid (added by Email.fetch).
        "archive" => |host, args| Box::pin(async move {
            let Some(conn) = email_conn_from_env(host.runtime().env.as_ref()) else {
                eprintln!("  ⚠ Email.archive: IMAP_HOST/EMAIL_USER/EMAIL_PASS not set — skipping");
                return Ok(Value::None);
            };
            let uid = match positional(&args, 0) {
                Some(Value::Map(m)) => match m.get(&MapKey::Str("uid".into())) {
                    Some(Value::Integer(u)) if *u > 0 => *u as u32,
                    _ => return Err(miette::miette!(
                        "Email.archive: message has no UID — was it returned by Email.fetch?"
                    )),
                },
                _ => return Err(miette::miette!("Email.archive: expected an email map argument")),
            };
            let folder = host.runtime().env.var("IMAP_ARCHIVE_FOLDER")
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Archive".to_string());
            match tokio::task::spawn_blocking(move || email::archive_email(&conn, uid, &folder)).await {
                Ok(Ok(())) => Ok(Value::None),
                Ok(Err(msg)) => Err(miette::miette!("{msg}")),
                Err(e) => Err(miette::miette!("email archive task join error: {e}")),
            }
        }),
    })
}

fn email_text_field<'a>(
    fields: &'a std::collections::HashMap<MapKey, Value>,
    field: &str,
    caller: &str,
) -> miette::Result<Option<&'a str>> {
    fields
        .get(&MapKey::Str(field.to_string()))
        .map(|value| expect_str_value(value, &format!("message.{field}"), caller))
        .transpose()
}

/// Build an `EmailConnection` from environment variables. Returns
/// `None` if required variables are missing (fetch/send then degrade
/// gracefully).
fn email_conn_from_env(env: &dyn context::EnvProvider) -> Option<email::EmailConnection> {
    let imap_host = env.var("IMAP_HOST").filter(|s| !s.is_empty())?;
    let user = env.var("EMAIL_USER").filter(|s| !s.is_empty())?;
    let pass = env.var("EMAIL_PASS").filter(|s| !s.is_empty())?;
    let smtp_host = env
        .var("SMTP_HOST")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| imap_host.replace("imap.", "smtp."));
    Some(email::EmailConnection {
        imap_host,
        smtp_host,
        user,
        pass,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_conn_from_env_requires_imap_user_and_password() {
        let env = context::MapEnv::with(&[
            ("IMAP_HOST", "imap.example.test"),
            ("EMAIL_USER", "bot@example.test"),
        ]);

        assert!(email_conn_from_env(&env).is_none());
    }

    #[test]
    fn email_conn_from_env_derives_smtp_host_from_imap_host() {
        let env = context::MapEnv::with(&[
            ("IMAP_HOST", "imap.example.test"),
            ("EMAIL_USER", "bot@example.test"),
            ("EMAIL_PASS", "secret"),
        ]);

        let conn = email_conn_from_env(&env).expect("email config");
        assert_eq!(conn.imap_host, "imap.example.test");
        assert_eq!(conn.smtp_host, "smtp.example.test");
        assert_eq!(conn.user, "bot@example.test");
        assert_eq!(conn.pass, "secret");
    }

    #[test]
    fn email_conn_from_env_uses_explicit_smtp_host() {
        let env = context::MapEnv::with(&[
            ("IMAP_HOST", "mail.example.test"),
            ("SMTP_HOST", "smtp-relay.example.test"),
            ("EMAIL_USER", "bot@example.test"),
            ("EMAIL_PASS", "secret"),
        ]);

        let conn = email_conn_from_env(&env).expect("email config");
        assert_eq!(conn.smtp_host, "smtp-relay.example.test");
    }
}
