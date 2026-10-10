{
  description = "Telegram video stickers and emoji from any video, and animated stickers from pixel art";

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
      # built with the Rust and ffmpeg of the nixpkgs it is added to,
      # overlays on them included
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

        # cargo, clippy and rustfmt from nixpkgs, ffmpeg's libraries for
        # `--features linked`, and what `cargo xtask ffmpeg` builds with
        devShells.default = pkgs.mkShell {
          inputsFrom = [ pkgs.tgradish ];
          packages = with pkgs; [
            cargo-about
            clippy
            curl
            rustfmt
            meson
            nasm
            ninja
            pkg-config
          ];
          buildInputs = [ pkgs.ffmpeg ];
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath pkgs.tgradish.windowLibraries;
        };
      }
    );
}
