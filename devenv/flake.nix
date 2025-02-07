{
  inputs = {
    devenv = {
      inputs.nixpkgs.follows = "nixpkgs";
      url = "github:cachix/devenv";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";
  };
  nixConfig = {
    extra-trusted-public-keys = "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw= nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs=";
    extra-substituters = "https://cache.nixos.org https://devenv.cachix.org https://nix-community.cachix.org";
  };
  outputs = {
    devenv,
    fenix,
    nixpkgs,
    self,
    systems,
    ...
  } @ inputs: let
    for-each-system = nixpkgs.lib.genAttrs (import systems);
  in {
    devShells = for-each-system (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        default = devenv.lib.mkShell {
          inherit inputs pkgs;
          modules = [
            ({
              config,
              ...
            }: {
              languages = {
                rust = {
                  channel = "stable";
                  enable = true;
                };
              };
            })
          ];
        };
      }
    );
  };
}
