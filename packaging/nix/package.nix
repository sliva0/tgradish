{
  lib,
  rustPlatform,
  makeWrapper,
  ffmpeg,
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
  # the window loads these with dlopen when it opens
  windowLibraries = [
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
  '';

  # after the fixup phase has shrunk the rpath to what is linked
  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath windowLibraries} $out/bin/tgradish
    wrapProgram $out/bin/tgradish --prefix PATH : ${lib.makeBinPath [ ffmpeg ]}
  '';

  passthru = { inherit windowLibraries; };

  meta = {
    description = "Telegram video stickers and emoji from any video, and animated stickers from pixel art";
    homepage = "https://github.com/sliva0/tgradish";
    license = lib.licenses.mit;
    mainProgram = "tgradish";
    platforms = lib.platforms.linux;
  };
}
