{
  description = "Telegram video stickers and emoji from any video, and animated stickers from pixel art";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
          system: f nixpkgs.legacyPackages.${system}
        );
    in
    {
      packages = forAllSystems (pkgs: rec {
        tgradish = pkgs.callPackage ./packaging/nix/package.nix { };
        default = tgradish;
      });

      checks = forAllSystems (pkgs: {
        tgradish = self.packages.${pkgs.stdenv.hostPlatform.system}.tgradish;
      });

      # cargo, clippy and rustfmt from nixpkgs, ffmpeg's libraries for
      # `--features linked`, and what `cargo xtask ffmpeg` builds with
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.tgradish ];
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
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath self.packages.${pkgs.stdenv.hostPlatform.system}.tgradish.windowLibraries;
        };
      });
    };
}
