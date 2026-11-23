//! System integration: opening folders and URLs in the user's file manager
//! or browser (avoiding eframe's heavier URL-opening dependency), and copying
//! files to the clipboard. Under `#[cfg(test)]`, opened URLs and copied files
//! are recorded instead.

use std::path::Path;

/// Opens the folder containing `path`, selecting the file when supported.
pub(crate) fn reveal(path: &Path) {
    #[cfg(target_os = "windows")]
    wait_for(std::process::Command::new("explorer").arg("/select,").arg(path).spawn());
    #[cfg(target_os = "macos")]
    wait_for(std::process::Command::new("open").arg("-R").arg(path).spawn());
    // Ask the file manager over D-Bus on a background thread (launching it
    // can take a while); fall back to opening the parent directory with
    // xdg-open if no service responds.
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

/// Formats an absolute `path` as a `file://` URI, percent-encoding any byte
/// outside ASCII alphanumeric and `-._~/`.
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

/// Opens an `http://` or `https://` URL in the system browser (ignoring other
/// strings so file paths or CLI flags cannot be passed). Telegram chat links
/// open in the desktop app when installed; see [`telegram`].
pub(crate) fn browse(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    // In tests, record the URL; `if cfg!(test)` keeps the rest reachable to
    // the compiler.
    #[cfg(test)]
    if cfg!(test) {
        BROWSED.lock().unwrap().push(url.to_owned());
        return;
    }
    match app_link(url) {
        // Probe and launch the Telegram app on a background thread because it
        // spawns and waits on external processes.
        Some(app) => {
            let web = url.to_owned();
            let _ = std::thread::Builder::new()
                .name("telegram".into())
                .spawn(move || telegram(&Host, &app, &web));
        }
        None => wait_for(opener(url).spawn()),
    }
}

/// Returns the system command to open `url`.
fn opener(url: &str) -> std::process::Command {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    let mut command = std::process::Command::new(program);
    command.arg(url);
    command
}

/// Converts a `https://t.me/` chat URL into a `tg:` deep link per
/// <https://core.telegram.org/api/links>: `t.me/name` becomes
/// `tg://resolve?domain=name`, and `t.me/name?direct` appends `&direct`.
/// Returns `None` for other `t.me` paths or non-Telegram URLs.
pub(crate) fn app_link(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://t.me/")?;
    let (name, query) = rest.split_once('?').unwrap_or((rest, ""));
    let name = name.strip_suffix('/').unwrap_or(name);
    let username = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if name.len() < 4 || !name.chars().all(username) {
        return None;
    }
    match query {
        "" => Some(format!("tg://resolve?domain={name}")),
        "direct" => Some(format!("tg://resolve?domain={name}&direct")),
        _ => None,
    }
}

/// Queries and opens `tg:` and web links on the host OS.
pub(crate) trait Telegram {
    /// Returns whether a handler for `tg:` URLs is registered, or `None` if
    /// the platform cannot tell ahead of time.
    fn takes_tg_links(&self) -> Option<bool>;
    /// Returns whether trying `tg:` blindly is safe when `takes_tg_links` is
    /// `None` (true when an unhandled scheme fails cleanly instead of showing
    /// an OS error dialog).
    fn may_try(&self) -> bool;
    /// Opens `url` and returns true if the launcher succeeded.
    fn open(&self, url: &str) -> bool;
}

/// Opens a Telegram link via `app` (`tg://...`) when a handler is registered
/// (or safe to try), falling back to `web` (`https://t.me/...`) if none is
/// registered or opening `app` fails. Something is always opened.
pub(crate) fn telegram(system: &dyn Telegram, app: &str, web: &str) {
    let tried = match system.takes_tg_links() {
        Some(takes) => takes,
        None => system.may_try(),
    };
    if !(tried && system.open(app)) {
        system.open(web);
    }
}

/// Host implementation of [`Telegram`].
struct Host;

impl Telegram for Host {
    /// On Linux/BSD, queries `xdg-mime` for `x-scheme-handler/tg` unless
    /// running inside a Flatpak or Snap sandbox (which cannot see host apps
    /// and routes links through the desktop portal). On Windows, checks the
    /// `tg` URL protocol in the registry. On macOS, returns `None` because
    /// `open` exits with an error when no app claims the scheme.
    fn takes_tg_links(&self) -> Option<bool> {
        #[cfg(target_os = "windows")]
        return Some(windows_takes_tg());
        #[cfg(target_os = "macos")]
        return None;
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            if sandboxed() {
                return None;
            }
            let asked = std::process::Command::new("xdg-mime")
                .args(["query", "default", "x-scheme-handler/tg"])
                .stderr(std::process::Stdio::null())
                .output()
                .ok()?;
            Some(
                asked.status.success() && !String::from_utf8_lossy(&asked.stdout).trim().is_empty(),
            )
        }
    }

    fn may_try(&self) -> bool {
        // Outside a sandbox, xdg-open fails cleanly like macOS `open` if
        // xdg-mime was unavailable.
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        return !sandboxed();
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        return cfg!(target_os = "macos");
    }

    fn open(&self, url: &str) -> bool {
        // Use ShellExecuteW on Windows because `explorer.exe` exits with code 1
        // even on success.
        #[cfg(target_os = "windows")]
        return windows_open(url);
        #[cfg(not(target_os = "windows"))]
        {
            let Ok(mut child) = opener(url).spawn() else { return false };
            child.wait().is_ok_and(|status| status.success())
        }
    }
}

/// Opens `url` via `ShellExecuteW` and returns whether it succeeded, so a
/// stale `tg:` registration whose binary was deleted falls back to the web URL.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn windows_open(url: &str) -> bool {
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(url));
    // SAFETY: Initialise apartment-threaded COM on this thread as required by
    // ShellExecuteW; both UTF-16 strings are NUL-terminated, and null HWND /
    // parameter pointers are valid.
    let opened = unsafe {
        let flags = (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32;
        CoInitializeEx(std::ptr::null(), flags);
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value > 32 on success.
    opened as usize > 32
}

/// Extracts the executable path from a Windows registry command string such as
/// `"C:\path\Telegram.exe" -- "%1"` or `C:\path\Telegram.exe -- %1`: takes the
/// first quoted string, or up to the first `.exe`, or the first whitespace-
/// delimited token.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn command_program(command: &str) -> Option<std::path::PathBuf> {
    let command = command.trim();
    let program = match command.strip_prefix('"') {
        Some(rest) => rest.split_once('"').map(|(program, _)| program)?,
        None => {
            let end = command.to_ascii_lowercase().find(".exe").map(|at| at + 4);
            end.map_or_else(
                || command.split_whitespace().next().unwrap_or(""),
                |end| &command[..end],
            )
        }
    };
    (!program.is_empty()).then(|| std::path::PathBuf::from(program))
}

/// Returns true if running inside a Flatpak or Snap sandbox.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
        || std::env::var_os("FLATPAK_ID").is_some()
        || std::env::var_os("SNAP").is_some()
}

/// Returns true if the `tg` URL protocol is registered in `HKEY_CLASSES_ROOT`
/// and its open command points to an existing file on disk (ignoring stale
/// keys left behind after uninstalling Telegram).
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn windows_takes_tg() -> bool {
    use windows_sys::Win32::System::Registry::{
        HKEY_CLASSES_ROOT, RRF_RT_ANY, RRF_RT_REG_SZ, RegGetValueW,
    };
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    // Returns the value's byte size, and writes its UTF-16 contents into
    // `data` when provided.
    let value = |key: &str, name: Option<&str>, data: Option<&mut Vec<u16>>| {
        let key = wide(key);
        let name = name.map(wide);
        let name = name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr());
        let mut size = data.as_ref().map_or(0, |data| (data.len() * 2) as u32);
        let (flags, pointer) = match data {
            Some(data) => (RRF_RT_REG_SZ, data.as_mut_ptr().cast()),
            None => (RRF_RT_ANY, std::ptr::null_mut()),
        };
        // SAFETY: `key` and `name` (when non-null) are NUL-terminated UTF-16,
        // and `size` matches the byte capacity of `pointer` (0 when null).
        let status = unsafe {
            RegGetValueW(
                HKEY_CLASSES_ROOT,
                key.as_ptr(),
                name,
                flags,
                std::ptr::null_mut(),
                pointer,
                &mut size,
            )
        };
        (status == 0).then_some(size)
    };
    let command_key = "tg\\shell\\open\\command";
    if value("tg", Some("URL Protocol"), None).is_none() {
        return false;
    }
    let Some(size) = value(command_key, None, None) else { return false };
    let mut text = vec![0u16; (size as usize).div_ceil(2).max(1)];
    if value(command_key, None, Some(&mut text)).is_none() {
        return false;
    }
    let end = text.iter().position(|&unit| unit == 0).unwrap_or(text.len());
    let command = String::from_utf16_lossy(&text[..end]);
    command_program(&command).is_some_and(|program| program.is_file())
}

/// Persistent clipboard handle kept alive for the lifetime of the GUI (on X11
/// and Wayland, clipboard contents vanish when the owning handle is dropped).
#[derive(Default)]
pub(crate) struct Clipboard(Option<arboard::Clipboard>);

impl Clipboard {
    /// Copies `path` to the clipboard as a file (URI list on Linux, `CF_HDROP`
    /// on Windows, file URL on macOS) for pasting into Telegram or a file
    /// manager.
    pub(crate) fn copy_file(&mut self, path: &Path) -> Result<(), String> {
        let path = std::path::absolute(path).map_err(|err| err.to_string())?;
        #[cfg(test)]
        {
            let _ = &mut self.0;
            COPIED.lock().unwrap().push(path);
            Ok(())
        }
        #[cfg(not(test))]
        {
            let clipboard = match &mut self.0 {
                Some(clipboard) => clipboard,
                None => self.0.insert(arboard::Clipboard::new().map_err(|err| err.to_string())?),
            };
            clipboard.set().file_list(&[path]).map_err(|err| err.to_string())
        }
    }
}

/// Files copied to the clipboard during tests.
#[cfg(test)]
pub(crate) static COPIED: std::sync::Mutex<Vec<std::path::PathBuf>> =
    std::sync::Mutex::new(Vec::new());

/// URLs opened during tests.
#[cfg(test)]
pub(crate) static BROWSED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Reaps a spawned launcher process on a background thread so it does not
/// remain a zombie.
fn wait_for(started: std::io::Result<std::process::Child>) {
    if let Ok(mut child) = started {
        std::thread::spawn(move || child.wait());
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::l10n::fl;
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
    fn opens_telegram_chats_in_the_app() {
        let direct = "tg://resolve?domain=s0untitled&direct";
        assert_eq!(app_link("https://t.me/s0untitled?direct").as_deref(), Some(direct));
        let resolve = "tg://resolve?domain=durov";
        assert_eq!(app_link("https://t.me/durov").as_deref(), Some(resolve));
        for other in ["https://t.me/+invite", "https://t.me/s/channel?x", "https://example.org"] {
            assert_eq!(app_link(other), None, "{other}");
        }
    }

    #[test]
    fn finds_the_program_of_a_registered_command() {
        let program = |command| command_program(command).map(|path| path.display().to_string());
        let quoted = r#""C:\Users\me\AppData\Roaming\Telegram Desktop\Telegram.exe" -- "%1""#;
        let expected = r"C:\Users\me\AppData\Roaming\Telegram Desktop\Telegram.exe";
        assert_eq!(program(quoted).as_deref(), Some(expected));
        assert_eq!(
            program(r"C:\Telegram\Telegram.exe -- %1").as_deref(),
            Some(r"C:\Telegram\Telegram.exe")
        );
        assert_eq!(program("telegram %1").as_deref(), Some("telegram"));
        assert_eq!(program("  "), None);
        assert_eq!(program(r#""unclosed"#), None);
    }

    /// A system that says `takes`, `may_try`s or not, whose opening of
    /// `tg:` links goes as `app_opens` and is noted.
    struct Pretend {
        takes: Option<bool>,
        may_try: bool,
        app_opens: bool,
        opened: std::cell::RefCell<Vec<String>>,
    }

    impl Telegram for Pretend {
        fn takes_tg_links(&self) -> Option<bool> {
            self.takes
        }
        fn may_try(&self) -> bool {
            self.may_try
        }
        fn open(&self, url: &str) -> bool {
            self.opened.borrow_mut().push(url.to_owned());
            !url.starts_with("tg:") || self.app_opens
        }
    }

    #[test]
    fn opens_telegram_or_else_the_web() {
        let (app, web) =
            ("tg://resolve?domain=s0untitled&direct", "https://t.me/s0untitled?direct");
        let opened = |takes, may_try, app_opens| {
            let system =
                Pretend { takes, may_try, app_opens, opened: std::cell::RefCell::default() };
            telegram(&system, app, web);
            system.opened.into_inner()
        };
        // a program takes them: the app, and only that where it opens
        assert_eq!(opened(Some(true), false, true), [app]);
        // the web where it fails, so something always opens
        assert_eq!(opened(Some(true), false, false), [app, web]);
        // none does: the web alone, not a dialog of the system's
        assert_eq!(opened(Some(false), true, true), [web]);
        // the system can't say: tried where failing shows, as on macOS
        assert_eq!(opened(None, true, true), [app]);
        assert_eq!(opened(None, true, false), [app, web]);
        // and not in a sandbox, which can't tell either way
        assert_eq!(opened(None, false, true), [web]);
    }

    #[test]
    fn opens_links() {
        let mut harness = harness(Config::default());
        harness.state_mut().dialogs.about = true;
        harness.run_steps(2);
        // a link, a label whose text is its name rather than its value
        let source = fl!("source-code");
        let link = egui_kittest::kittest::by()
            .predicate(|node| node.label().as_deref() == Some(source.as_str()));
        harness.get(link).click_accesskit();
        harness.run_steps(2);
        let browsed = BROWSED.lock().unwrap();
        assert!(
            browsed.iter().any(|url| url == "https://github.com/sliva0/tgradish"),
            "{browsed:?}"
        );
        drop(browsed);
        harness.get_by_label(&fl!("send-feedback")).click_accesskit();
        harness.run_steps(2);
        let browsed = BROWSED.lock().unwrap();
        assert!(browsed.iter().any(|url| url == "https://t.me/s0untitled?direct"), "{browsed:?}");
        drop(browsed);
        // and from the menu over the list, beside the way to About
        let before = BROWSED.lock().unwrap().len();
        harness.state_mut().dialogs.about = false;
        harness.run_steps(2);
        harness.get_by_label(&fl!("main-menu")).click();
        harness.run_steps(2);
        harness.get_by_label(&fl!("feedback")).click();
        harness.run_steps(2);
        let browsed = BROWSED.lock().unwrap();
        assert_eq!(browsed[before..], ["https://t.me/s0untitled?direct"]);
    }
}
