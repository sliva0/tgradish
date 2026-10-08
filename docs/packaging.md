# Packaging: AUR and Nix

Everything the packages need is in this repository; publishing them needs
your accounts, so these are the steps for you. Do them after the 2.0.0
release is published (not a draft), since the packages download from it.

## What there is

- `packaging/aur/tgradish/PKGBUILD`: builds from the release's source
  tarball, against the system ffmpeg (`depends=(ffmpeg)`).
- `packaging/aur/tgradish-bin/PKGBUILD`: the release's
  `*-system-ffmpeg` Linux builds for x86-64 and aarch64, which also use the
  system ffmpeg, so both packages behave the same.
- `flake.nix` and `packaging/nix/package.nix`: a Nix package built from
  source against nixpkgs' ffmpeg, and a development shell.
- Both install `tgradish` and a menu entry that opens the window
  (`tgradish.desktop`).

The PKGBUILDs' maintainer line has your e-mail address written the usual
AUR way (`sliva0mk at gmail dot com`); change it if you'd rather not
publish it.

## AUR

### Once

1. Make an account at <https://aur.archlinux.org/register>.
2. Make an SSH key for it and add the public key under "My Account":

   ```console
   ssh-keygen -t ed25519 -f ~/.ssh/aur
   ```

3. Tell SSH to use it, in `~/.ssh/config`:

   ```
   Host aur.archlinux.org
     IdentityFile ~/.ssh/aur
     User aur
   ```

4. Install the tools: `sudo pacman -S --needed base-devel pacman-contrib
   namcap devtools`.

### Each release

For each of `tgradish` and `tgradish-bin`, in
`packaging/aur/<package>/`:

1. Set `pkgver` to the release's version and `pkgrel=1`.
2. Fill in the checksums, which are `SKIP` until there is a release to
   download: `updpkgsums`.
3. Build and check it in a clean chroot, which catches missing
   dependencies: `extra-x86_64-build` (from devtools), then
   `namcap PKGBUILD *.pkg.tar.zst`. A quicker check on your own system is
   `makepkg -f`.
4. Regenerate the metadata the AUR reads:
   `makepkg --printsrcinfo > .SRCINFO`.
5. Commit the PKGBUILD and `.SRCINFO` here.

Then publish. The first push creates the package; cloning a name nobody
has taken gives an empty repository:

```console
git clone ssh://aur@aur.archlinux.org/tgradish.git /tmp/aur-tgradish
cp packaging/aur/tgradish/{PKGBUILD,.SRCINFO} /tmp/aur-tgradish/
cd /tmp/aur-tgradish
git add PKGBUILD .SRCINFO
git commit -m "tgradish 2.0.0"
git push
```

Do the same with `tgradish-bin`. The AUR only accepts the `master` branch,
and only `PKGBUILD`, `.SRCINFO` and files the PKGBUILD uses.

For later releases, it is the same steps, starting from step 1, and a
push to the existing clone.

## Nix

The flake works as soon as it is on GitHub, nothing to publish:

```console
# run it
nix run github:sliva0/tgradish
# install it
nix profile install github:sliva0/tgradish
# work on tgradish: cargo, clippy, ffmpeg, and what `cargo xtask ffmpeg` needs
nix develop
```

In a NixOS configuration, add the flake as an input and put
`inputs.tgradish.packages.${pkgs.stdenv.hostPlatform.system}.default` in
`environment.systemPackages`.

- The `nix` workflow builds the flake on every push that changes it or
  `Cargo.lock`, and runs the tests.
- Update nixpkgs now and then with `nix flake update`, and commit
  `flake.lock`.
- Outside NixOS, Nix programs can't find the system's OpenGL drivers, so
  the window needs [nixGL](https://github.com/nix-community/nixGL)
  there; the command line works as is.

### nixpkgs, later

Getting tgradish into nixpkgs itself (so `nix-shell -p tgradish` works
without the flake) means a pull request to NixOS/nixpkgs:

1. Add yourself to `maintainers/maintainer-list.nix` if you aren't there.
2. Copy `packaging/nix/package.nix` to `pkgs/by-name/tg/tgradish/package.nix`
   and change `src` to `fetchFromGitHub` with the release tag, `cargoLock`
   to `cargoHash`, and `version` to the release's.
3. Build it with `nix-build -A tgradish`, then open the pull request.
