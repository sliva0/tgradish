{
  description = "Telegram video, static and animated stickers and emoji from videos, images or pixel art";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    {
      # Builds tgradish with the caller's nixpkgs Rust and ffmpeg (including
      # overlays).
      overlays.default = final: prev: {
        tgradish = final.callPackage ./packaging/nix/package.nix { };
      };
    }
    // flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          localSystem = { inherit system; };
          overlays = [ self.overlays.default ];
        };
      in
      {
        packages = rec {
          inherit (pkgs) tgradish;
          default = tgradish;
        };

        checks = {
          inherit (pkgs) tgradish;
        };

        # Provides cargo, clippy and rustfmt from nixpkgs, ffmpeg dev libraries
        # for `--features linked`, build tools for `cargo xtask ffmpeg`, and
        # cmake for tgs-lab's rlottie. Omits cargo-about because
        # `cargo xtask licenses` requires exact version 0.9.2.
        devShells.default = pkgs.mkShell {
          inputsFrom = [ pkgs.tgradish ];
          packages = with pkgs; [
            clippy
            cmake
            curl
            rustfmt
            meson
            nasm
            ninja
            pkg-config
          ];
          buildInputs = [ pkgs.ffmpeg ];
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath pkgs.tgradish.guiLibraries;
        };
      }
    );
}
