//! Folders and web addresses opened with the system's own programs, as a
//! file manager or browser would open them; eframe opens links only with
//! a dependency much larger than this. Tests note the addresses instead.

use std::path::Path;

/// Opens the folder holding `path`, with the file selected where the
/// system can do that.
pub(crate) fn reveal(path: &Path) {
    #[cfg(target_os = "windows")]
    wait_for(std::process::Command::new("explorer").arg("/select,").arg(path).spawn());
    #[cfg(target_os = "macos")]
    wait_for(std::process::Command::new("open").arg("-R").arg(path).spawn());
    // the file manager is asked over D-Bus, which may start it and so takes
    // a while; the folder alone where none answers
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let path = path.to_path_buf();
        let _ = std::thread::Builder::new().name("reveal".into()).spawn(move || {
            let selected = path
                .canonicalize()
                .ok()
                .and_then(|path| std::ffi::CString::new(file_uri(&path)).ok())
                .is_some_and(|uri| crate::dbus::show_items(&uri));
            if !selected {
                let folder =
                    path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
                if let Ok(mut child) = std::process::Command::new("xdg-open").arg(folder).spawn() {
                    let _ = child.wait();
                }
            }
        });
    }
}

/// `path`, absolute, as a file:// address, its bytes beyond letters,
/// digits and `-._~/` escaped.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
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
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fn names_files_as_addresses() {
        assert_eq!(
            file_uri(Path::new("/tmp/a b/ü,1%.webm")),
            "file:///tmp/a%20b/%C3%BC%2C1%25.webm"
        );
    }

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
