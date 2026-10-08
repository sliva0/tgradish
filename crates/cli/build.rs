fn main() {
    println!("cargo::rerun-if-changed=tgradish.exe.manifest");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest::embed_manifest_file("tgradish.exe.manifest")
            .expect("the Windows manifest should embed");
    }
}
