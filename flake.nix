{
  description = "Zoeken — privacy-respecting metasearch (Rust + SPA)";

  # Track a channel/branch here; flake.lock pins the exact nixpkgs revision.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

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

      # Consumers should depend on github:Greenstorm5417/Zoeken (no /vX.Y.Z in the
      # flake URL). Their flake.lock pins the Zoeken revision — and with it the
      # prebuilt release pointed at by packaging/nix/generated.nix on that rev.
      #
      #   nix run github:Greenstorm5417/Zoeken
      #   nix profile install github:Greenstorm5417/Zoeken

      formatter = forAllSystems (system: (import nixpkgs { inherit system; }).nixfmt-rfc-style);
    };
}
