{
  description = "Zoeken — privacy-respecting metasearch (Rust + SPA)";

  # Same pin as packaging/nix (NixOS 26.05).
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/fd1462031fdee08f65fd0b4c6b64e22239a77870";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      overlays.default = final: prev: {
        zoeken = final.callPackage ./packaging/nix/zoeken.nix { };
      };

      packages = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ self.overlays.default ];
          };
        in
        {
          default = pkgs.zoeken;
          zoeken = pkgs.zoeken;
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.zoeken}/bin/zoeken-server";
        };
        zoeken = self.apps.${system}.default;
      });

      checks = forAllSystems (system: {
        zoeken = self.packages.${system}.zoeken;
      });

      #   nix run github:Greenstorm5417/Zoeken
      #   nix profile install github:Greenstorm5417/Zoeken

      formatter = forAllSystems (system: (import nixpkgs { inherit system; }).nixfmt-rfc-style);
    };
}
