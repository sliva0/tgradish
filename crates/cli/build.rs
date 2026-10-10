fn main() {
    for file in ["tgradish.rc", "tgradish.exe.manifest", "assets/tgradish.ico"] {
        println!("cargo::rerun-if-changed={file}");
    }
    // with rc.exe from the Windows SDK, or MinGW's windres
    embed_resource::compile("tgradish.rc", embed_resource::NONE)
        .manifest_required()
        .expect("the Windows resources should compile");
}
