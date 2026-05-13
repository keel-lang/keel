//! Project scaffolding for `keel init`.

use miette::Result;
use std::fs;
use std::path::PathBuf;

pub fn project(name: Option<String>) -> Result<()> {
    let (project_name, dir, in_place) = match name {
        Some(n) => {
            let dir = PathBuf::from(&n);
            if dir.exists() {
                return Err(miette::miette!("Directory '{}' already exists", n));
            }
            fs::create_dir_all(&dir)
                .map_err(|e| miette::miette!("Failed to create directory: {e}"))?;
            let project_name = dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(n);
            (project_name, dir, false)
        }
        None => {
            let cwd = std::env::current_dir()
                .map_err(|e| miette::miette!("Cannot read current directory: {e}"))?;
            let project_name = cwd
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "my_agent".to_string());
            (project_name, cwd, true)
        }
    };

    let main_keel_path = dir.join("main.keel");
    if main_keel_path.exists() {
        return Err(miette::miette!("main.keel already exists"));
    }

    let main_keel = format!(
        r#"# {project_name} — built with Keel

agent {agent_name} {{
  @role "Describe what this agent does"

  @on_start {{
    Io.show("Hello from {project_name}!")
    stop(self)
  }}
}}

run({agent_name})
"#,
        agent_name = to_pascal_case(&project_name)
    );
    fs::write(&main_keel_path, main_keel)
        .map_err(|e| miette::miette!("Failed to write main.keel: {e}"))?;

    let gitignore_path = dir.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(gitignore_path, "*.log\n.env\n")
            .map_err(|e| miette::miette!("Failed to write .gitignore: {e}"))?;
    }

    eprintln!("✓ Initialized project '{project_name}'");
    if in_place {
        eprintln!("  main.keel");
        eprintln!();
        eprintln!("  Run it:  keel run main.keel");
    } else {
        eprintln!("  {}/main.keel", project_name);
        eprintln!();
        eprintln!("  Run it:  keel run {}/main.keel", project_name);
    }
    Ok(())
}

fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::to_pascal_case;

    #[test]
    fn converts_common_project_names_to_pascal_case() {
        assert_eq!(to_pascal_case("mail-agent"), "MailAgent");
        assert_eq!(to_pascal_case("daily_digest"), "DailyDigest");
        assert_eq!(to_pascal_case("  ai scout  "), "AiScout");
    }
}
