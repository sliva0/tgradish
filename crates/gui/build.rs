//! Builds the GUI again whenever anything under `i18n/` changes. The
//! translations there are embedded at compile time, and cargo would
//! otherwise notice only edits to the files embedded last time, not a
//! language's file or directory added since, which the build would then
//! silently go without. Cargo scans a directory named here as a whole,
//! so a new file or directory in it counts as a change.

fn main() {
    println!("cargo::rerun-if-changed=i18n");
    println!("cargo::rerun-if-changed=i18n.toml");
}
