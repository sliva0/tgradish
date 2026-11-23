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
  # Libraries dlopened by the GUI at runtime (graphics and D-Bus file
  # dialogues). Keep in sync with GUI_LIBRARIES in xtask/src/package.rs, which
  # documents where each comes from.
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
      # Exclude tgs-lab (separate workspace not built by this package).
      (lib.fileset.difference ../../crates ../../crates/tgs-lab)
      ../../xtask
    ];
  };

  cargoLock.lockFile = ../../Cargo.lock;

  cargoBuildFlags = [ "--package=tgradish" ];
  cargoTestFlags = [
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
  # Generate completions and man pages by running the built binary (skipped
  # when cross-compiling).
  + lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
    installShellCompletion --cmd tgradish \
      --bash <($out/bin/tgradish completions bash) \
      --zsh <($out/bin/tgradish completions zsh) \
      --fish <($out/bin/tgradish completions fish) \
      --nushell <($out/bin/tgradish completions nushell)
    $out/bin/tgradish __man man
    installManPage man/*.1
  '';

  # Run after fixup so patchelf does not strip our dlopen rpath entries.
  postFixup = ''
    patchelf --add-rpath ${lib.makeLibraryPath guiLibraries} $out/bin/tgradish
    wrapProgram $out/bin/tgradish --prefix PATH : ${lib.makeBinPath [ ffmpeg ]}
  '';

  passthru = { inherit guiLibraries; };

  meta = {
    description = "Telegram video, static and animated stickers and emoji from videos, images or pixel art";
    homepage = "https://github.com/sliva0/tgradish";
    # libwebp (BSD-3-Clause) is in every build, and the GUI's fonts: Ubuntu
    # (Ubuntu Font Licence 1.0), JetBrains Mono (OFL-1.1), emoji-icon-font
    # (MIT).
    license = [
      lib.licenses.mit
      lib.licenses.bsd3
      lib.licenses.ufl
      lib.licenses.ofl
    ];
    mainProgram = "tgradish";
    platforms = lib.platforms.linux;
  };
}
