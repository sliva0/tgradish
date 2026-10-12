//! Folders and web addresses opened with the system's own programs, as a
//! file manager or browser would open them; eframe opens links only with
//! a dependency much larger than this. Tests note the addresses instead.

use std::path::Path;

/// Opens the folder holding `path`, with the file selected where the
/// system can do that.
pub(crate) fn reveal(path: &Path) {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    #[cfg(target_os = "windows")]
    let command = std::process::Command::new("explorer").arg("/select,").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let command = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let command = {
        let _ = path;
        std::process::Command::new("xdg-open").arg(folder).spawn()
    };
    let _ = folder;
    wait_for(command);
}

/// Opens a web address in the system's browser; anything else, which
/// could be a file or an option, is left alone.
pub(crate) fn browse(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    // tests only note it; the condition keeps what follows reachable
    #[cfg(test)]
    if cfg!(test) {
        BROWSED.lock().unwrap().push(url.to_owned());
        return;
    }
    #[cfg(target_os = "windows")]
    let command = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let command = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let command = std::process::Command::new("xdg-open").arg(url).spawn();
    wait_for(command);
}

/// Web addresses tests asked to open, rather than opening them.
#[cfg(test)]
pub(crate) static BROWSED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Waits for a program started to open something to end, on a thread of
/// its own, so it doesn't linger as a zombie.
fn wait_for(started: std::io::Result<std::process::Child>) {
    if let Ok(mut child) = started {
        std::thread::spawn(move || child.wait());
    }
}

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use tgradish_core::config::Config;

    use super::*;
    use crate::testing::harness;

    #[test]
    fn opens_links() {
        let mut harness = harness(Config::default());
        harness.state_mut().prefs.about = true;
        harness.run_steps(2);
        // a link, a label whose text is its name rather than its value
        let link = egui_kittest::kittest::by()
            .predicate(|node| node.label().as_deref() == Some("source code"));
        harness.get(link).click_accesskit();
        harness.run_steps(2);
        let browsed = BROWSED.lock().unwrap();
        assert!(
            browsed.iter().any(|url| url == "https://github.com/sliva0/tgradish"),
            "{browsed:?}"
        );
    }
}
