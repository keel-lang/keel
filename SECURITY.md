# Security Policy

Keel is **alpha software (v0.2)** with no production users and no stable API. We still take
security seriously — especially because the standard library touches LLM providers, email
(IMAP/SMTP), the filesystem, and the network, and programs may handle API keys and credentials.

## Supported versions

Keel ships frequent breaking `0.x` releases. Only the **latest released version** on the `main`
branch receives security fixes. There are no long-term support branches during the `0.x` series.

| Version | Supported |
| --- | --- |
| Latest `0.x` release | ✅ |
| Older `0.x` releases | ❌ |

## Reporting a vulnerability

**Please do not open a public issue for security vulnerabilities.**

Report privately through either of:

- **GitHub Security Advisories** — [open a private report](https://github.com/keel-lang/keel/security/advisories/new)
  (Security → Advisories → *Report a vulnerability*). This is the preferred channel.
- **Email** — `maktouf.zied@gmail.com` with the subject line `SECURITY: <short summary>`.

Please include:

- A description of the vulnerability and its impact.
- A minimal reproduction (a `.keel` program and/or commands) where possible.
- The Keel version (`keel --version`), OS, and toolchain.
- Any suggested remediation, if you have one.

## What to expect

- **Acknowledgement** within **5 business days**.
- An initial assessment and severity rating shortly after.
- We'll keep you updated on progress toward a fix and coordinate a disclosure timeline with you.
- With your permission, we'll credit you in the advisory and release notes once a fix ships.

## Scope

Examples of issues we want to hear about:

- Sandbox or capability escapes (a program reaching resources it wasn't granted).
- Credential or API-key leakage (e.g. keys surfacing in logs, traces, or error messages).
- Injection through LLM prompts/responses that leads to unintended file, network, or email actions.
- Memory-safety or panics reachable from untrusted `.keel` input that lead to DoS.

Out of scope: vulnerabilities in third-party LLM providers, mail servers, or dependencies (report
those upstream), and theoretical issues without a practical attack path.

Thank you for helping keep Keel and its users safe. ⚓
