//! `ghs init`, `ghs add`, `ghs remove` — config lifecycle without editing TOML.

use std::path::Path;

use color_eyre::eyre::{eyre, Result};

use crate::config::Config;
use crate::domain::Project;

/// `ghs init` — scaffold a config file.
pub fn init(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(eyre!(
            "config already exists at {} (use --force to overwrite)",
            path.display()
        ));
    }
    Config::write_scaffold(path)?;
    println!("✓ Wrote empty config to {}", path.display());
    println!("\nNow add some repositories:");
    println!("  ghs add owner/repo      # track a specific repo");
    println!("  ghs import --local      # discover your local git clones");
    println!("  ghs import --me         # import your GitHub repos");
    Ok(())
}

/// `ghs add owner/repo …` — append repos, creating the config if needed.
pub fn add(path: &Path, specs: &[String]) -> Result<()> {
    let (mut config, path) = Config::load_or_default(Some(path))?;

    let mut added = 0;
    for spec in specs {
        match Project::parse(spec) {
            Ok(project) => {
                let slug = project.slug();
                if config.add_project(project) {
                    println!("✓ added {slug}");
                    added += 1;
                } else {
                    println!("· {slug} already tracked");
                }
            }
            Err(e) => println!("✗ skipped '{spec}': {e}"),
        }
    }

    if added > 0 {
        config.save(&path)?;
        println!(
            "\nSaved {} ({added} new). Run `ghs` to view.",
            path.display()
        );
    } else {
        println!("\nNothing to add.");
    }
    Ok(())
}

/// `ghs remove <spec> …` — drop repos from the config.
///
/// Each `spec` is either:
///   * `owner/repo` — removes that single repository, or
///   * `owner`      — removes **every** repository of that owner/company.
pub fn remove(path: &Path, specs: &[String]) -> Result<()> {
    let (mut config, path) = Config::load_or_default(Some(path))?;

    let mut removed = 0;
    for spec in specs {
        let spec = spec.trim();
        if spec.contains('/') {
            // Single repo.
            match Project::parse(spec) {
                Ok(project) => {
                    if config.remove_project(&project.owner, &project.repo) {
                        println!("✓ removed {}", project.slug());
                        removed += 1;
                    } else {
                        println!("· {} was not tracked", project.slug());
                    }
                }
                Err(e) => println!("✗ skipped '{spec}': {e}"),
            }
        } else if spec.is_empty() {
            continue;
        } else {
            // Owner-only: remove the whole company.
            let n = config.remove_owner(spec);
            if n > 0 {
                println!("✓ removed {n} repo(s) of \"{spec}\"");
                removed += n;
            } else {
                println!("· no repos of \"{spec}\" were tracked");
            }
        }
    }

    if removed > 0 {
        config.save(&path)?;
        println!("\nSaved {} ({removed} removed).", path.display());
    } else {
        println!("\nNothing to remove.");
    }
    Ok(())
}
