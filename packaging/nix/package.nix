{
  lib,
  rustPlatform,
  makeWrapper,
  ffmpeg,
  dbus,
  libGL,
  libxkbcommon,
  wayland,
  libx11,
  libxcursor,
  libxi,
  libxrandr,
}:

let
  version = (lib.importTOML ../../Cargo.toml).workspace.package.version;
  # the window loads these with dlopen when it opens, and D-Bus for file
  # dialogues
  windowLibraries = [
    dbus
    libGL
    libxkbcommon
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
  ];
in
rustPlatform.buildRustPackage {
  pname = "tgradish";
  inherit version;

  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ../../Cargo.toml
      ../../Cargo.lock
      ../../crates
      ../../xtask
      # licence notices the binary carries, see crates/core/build.rs
      ../../LICENSE.txt
      ../../licenses
    ];
  };

  cargoLock = {
    lockFile = ../../Cargo.lock;
    # tlottie, which only the unpublished tgs-lab uses
    allowBuiltinFetchGit = true;
  };

  cargoBuildFlags = [ "--package=tgradish" ];
  cargoTestFlags = [
    "--package=tgradish-frames"
    "--package=tgradish-tgs"
    "--package=tgradish-core"
    "--package=tgradish"
  ];

  nativeBuildInputs = [ makeWrapper ];
  nativeCheckInputs = [ ffmpeg ];

  postInstall = ''
    install -Dm644 crates/cli/assets/tgradish.desktop -t $out/share/applications
    cp -r crates/cli/assets/icons $out/share/
  '';

  # after the fixup phase has shrunk the rpath to what is linked
  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath windowLibraries} $out/bin/tgradish
    wrapProgram $out/bin/tgradish --prefix PATH : ${lib.makeBinPath [ ffmpeg ]}
  '';

  passthru = { inherit windowLibraries; };

  meta = {
    description = "Telegram video and image stickers and emoji from any video or picture, and animated stickers from pixel art";
    homepage = "https://github.com/sliva0/tgradish";
    # libwebp is in every build
    license = [
      lib.licenses.mit
      lib.licenses.bsd3
    ];
    mainProgram = "tgradish";
    platforms = lib.platforms.linux;
  };
}
