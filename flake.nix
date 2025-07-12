{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };
  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      lib = pkgs.lib;
      ddlm = (
        pkgs.rustPlatform.buildRustPackage rec {
          pname = "ddlm";
          version = "1.0";
          src = ./.;
          cargoHash = "sha256-9DltXjD7AxRNCg/TWO4xYnIppJfWLoPgRJ0/u7IA+VM=";

          meta = {
            mainProgram = pname;
          };
        }
      );
    in
    {
      nixosConfigurations.default = nixpkgs.lib.nixosSystem {
        modules = [
          {
            nixpkgs.hostPlatform = system;
            services.greetd = {
              enable = true;
              settings = {
                default_session = {
                  command = lib.mkForce "${ddlm}/bin/ddlm --target ${pkgs.swayfx}/bin/swayfx";
                  user = "greeter";
                };
              };
            };
            users.users.greeter = {
              extraGroups = [
                "video"
                "input"
                "render"
              ];
            };
          }
        ];
      };
    };
}
