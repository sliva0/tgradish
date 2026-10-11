//! The system's accent colour, where it has one: from the desktop portal on
//! Linux, which Plasma 6 and GNOME 47 and later answer, and from the
//! registry on Windows. Read on a thread, as the portal may take a while to
//! start, and followed as it changes.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32};

pub struct Accent {
    colour: Arc<Mutex<Option<Color32>>>,
}

impl Accent {
    /// Starts reading the accent; `ctx` repaints when it arrives or changes.
    pub fn watch(ctx: &egui::Context) -> Accent {
        let colour = Arc::new(Mutex::new(None));
        let shared = colour.clone();
        let ctx = ctx.clone();
        let set = move |accent: Option<Color32>| {
            let mut colour = shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if *colour != accent {
                *colour = accent;
                ctx.request_repaint();
            }
        };
        // nothing to do without one: the window keeps Breeze's
        let _ = std::thread::Builder::new().name("accent".into()).spawn(move || follow(set));
        Accent { colour }
    }

    pub fn get(&self) -> Option<Color32> {
        *self.colour.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn follow(set: impl Fn(Option<Color32>)) {
    use zbus::zvariant::OwnedValue;

    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "accent-color";
    let Ok(connection) = zbus::blocking::Connection::session() else { return };
    let Ok(portal) = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    ) else {
        return;
    };
    // listening first, so a change while reading isn't missed
    let changes = portal.receive_signal("SettingChanged");
    // portals before 1.15 only have Read, which wraps the value once more
    let value = portal
        .call::<_, _, OwnedValue>("ReadOne", &(NAMESPACE, KEY))
        .or_else(|_| portal.call::<_, _, OwnedValue>("Read", &(NAMESPACE, KEY)));
    if let Ok(value) = value {
        set(colour(&value));
    }
    for message in changes.into_iter().flatten() {
        let Ok((namespace, key, value)) =
            message.body().deserialize::<(String, String, OwnedValue)>()
        else {
            continue;
        };
        if namespace == NAMESPACE && key == KEY {
            set(colour(&value));
        }
    }
}

/// The portal's accent: red, green and blue from 0 to 1, out of that range
/// when there is none.
#[cfg(all(unix, not(target_os = "macos")))]
fn colour(value: &zbus::zvariant::Value) -> Option<Color32> {
    use zbus::zvariant::Value;
    match value {
        Value::Value(inner) => colour(inner),
        Value::Structure(rgb) => {
            let channel = |value: &Value| match value {
                Value::F64(channel) if (0.0..=1.0).contains(channel) => {
                    Some((channel * 255.0).round() as u8)
                }
                _ => None,
            };
            match rgb.fields() {
                [red, green, blue] => {
                    Some(Color32::from_rgb(channel(red)?, channel(green)?, channel(blue)?))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Reads the accent every few seconds: Windows says when it changes only
/// in window messages, which winit doesn't pass on.
#[cfg(windows)]
fn follow(set: impl Fn(Option<Color32>)) {
    loop {
        set(registry_accent());
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

/// `AccentColor` of the window manager, a DWORD of 0xAABBGGRR.
#[cfg(windows)]
#[allow(unsafe_code)]
fn registry_accent() -> Option<Color32> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (key, name) = (wide("Software\\Microsoft\\Windows\\DWM"), wide("AccentColor"));
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: both names end in a NUL, and `data` has room for the DWORD
    // that RRF_RT_REG_DWORD allows
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &mut size,
        )
    };
    let [red, green, blue, _] = data.to_le_bytes();
    (status == 0).then(|| Color32::from_rgb(red, green, blue))
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn follow(_set: impl Fn(Option<Color32>)) {}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use zbus::zvariant::{StructureBuilder, Value};

    use super::*;

    #[test]
    fn reads_the_portal_accent() {
        let rgb = |r: f64, g: f64, b: f64| {
            Value::Structure(
                StructureBuilder::new().add_field(r).add_field(g).add_field(b).build().unwrap(),
            )
        };
        assert_eq!(colour(&rgb(1.0, 0.5, 0.0)), Some(Color32::from_rgb(255, 128, 0)));
        // wrapped once more, as some portals answer
        let wrapped = Value::Value(Box::new(rgb(0.0, 0.0, 1.0)));
        assert_eq!(colour(&wrapped), Some(Color32::from_rgb(0, 0, 255)));
        // out of range: no accent set
        assert_eq!(colour(&rgb(-1.0, -1.0, -1.0)), None);
        assert_eq!(colour(&Value::from(3u32)), None);
    }
}
