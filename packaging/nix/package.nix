# Back-compat entry for release CI (`nix-build packaging/nix/package.nix`).
# Prefer packaging/nix/from-context.nix or the flake package (zoeken.nix).
{ src, version ? "0.0.0" }:

let
  nixpkgs = builtins.fetchTarball
    "https://github.com/NixOS/nixpkgs/archive/fd1462031fdee08f65fd0b4c6b64e22239a77870.tar.gz";
  pkgs = import nixpkgs { };
in
pkgs.callPackage ./from-context.nix {
  inherit src version;
}
