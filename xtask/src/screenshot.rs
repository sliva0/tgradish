//! The README's picture of the window, rendered by one of the window's
//! tests from the synthetic art, so it can be made again as the window
//! changes.

use anyhow::{Context, Result, bail};

use crate::{cargo, run, workspace_root};

/// Renders the window at work to `.github/screenshot.png`. Needs a GPU
/// wgpu can use.
pub fn screenshot(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        bail!("usage: cargo xtask screenshot");
    }
    let root = workspace_root();
    let dir = root.join("target/screenshots");
    std::fs::create_dir_all(&dir)?;
    run(cargo()
        .current_dir(root)
        .args(["test", "-p", "tgradish-gui", "--lib", "--", "--ignored", "--exact"])
        .arg("screenshots::readme_screenshot")
        .env("TGRADISH_SCREENSHOTS", &dir))?;
    let rendered = dir.join("tgradish-readme.png");
    let to = root.join(".github/screenshot.png");
    std::fs::copy(&rendered, &to)
        .with_context(|| format!("could not copy {}", rendered.display()))?;
    println!("{}", to.display());
    Ok(())
}
