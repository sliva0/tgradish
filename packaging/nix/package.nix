{
  lib,
  rustPlatform,
  stdenv,
  makeWrapper,
  installShellFiles,
  ffmpeg,
  dbus,
  libGL,
  libxkbcommon,
  wayland,
  libx11,
  libxcb,
  libxcursor,
  libxi,
  libxrender,
}:

let
  version = (lib.importTOML ../../Cargo.toml).workspace.package.version;
  # the GUI loads these with dlopen when it opens, and D-Bus for file
  # dialogues; they are GUI_LIBRARIES in xtask/src/package.rs, which
  # says where each comes from: keep them alike
  guiLibraries = [
    dbus
    libGL
    libxkbcommon
    wayland
    libx11
    libxcb
    libxcursor
    libxi
    libxrender
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
      # tgs-lab is a workspace of its own, which the package doesn't build
      (lib.fileset.difference ../../crates ../../crates/tgs-lab)
      ../../xtask
    ];
  };

  cargoLock.lockFile = ../../Cargo.lock;

  cargoBuildFlags = [ "--package=tgradish" ];
  cargoTestFlags = [
    "--package=tgradish-frames"
    "--package=tgradish-tgs"
    "--package=tgradish-core"
    "--package=tgradish"
  ];

  nativeBuildInputs = [
    makeWrapper
    installShellFiles
  ];
  nativeCheckInputs = [ ffmpeg ];

  postInstall = ''
    install -Dm644 crates/cli/assets/tgradish.desktop -t $out/share/applications
    cp -r crates/cli/assets/icons $out/share/
  ''
  # written by the program, which a cross build can't run
  + lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd tgradish \
      --bash <($out/bin/tgradish completions bash) \
      --zsh <($out/bin/tgradish completions zsh) \
      --fish <($out/bin/tgradish completions fish) \
      --nushell <($out/bin/tgradish completions nushell)
    $out/bin/tgradish __man man
    installManPage man/*.1
  '';

  # after the fixup phase has shrunk the rpath to what is linked
  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath guiLibraries} $out/bin/tgradish
    wrapProgram $out/bin/tgradish --prefix PATH : ${lib.makeBinPath [ ffmpeg ]}
  '';

  passthru = { inherit guiLibraries; };

  meta = {
    description = "Telegram video, image and animated stickers and emoji from videos, images or pixel art";
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
