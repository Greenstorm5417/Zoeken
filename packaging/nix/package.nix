# Back-compat entry for release CI (`nix-build packaging/nix/package.nix`).
# Prefer packaging/nix/from-context.nix or the flake package (zoeken.nix).
# Uses the same nixpkgs channel as flake.nix; flake.lock is the pin for flakes.
{ src, version ? "0.0.0" }:

let
  nixpkgs = builtins.fetchTarball "https://github.com/NixOS/nixpkgs/archive/nixos-26.05.tar.gz";
  pkgs = import nixpkgs { };
in
pkgs.callPackage ./from-context.nix {
  inherit src version;
}
