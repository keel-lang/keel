//! REPL — placeholder for v0.1.
//!
//! Returns immediately with a migration-in-progress message. The full
//! REPL will be re-enabled once the interpreter lands on the new AST.

use miette::Result;

pub async fn start() -> Result<()> {
    eprintln!(
        "Keel REPL is unavailable in v0.1 alpha — interpreter migration \
         to namespace dispatch is in progress (see ROADMAP.md)"
    );
    Ok(())
}
