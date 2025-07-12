{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };
  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      lib = pkgs.lib;
      ndlm = (
        pkgs.rustPlatform.buildRustPackage rec {
          pname = "ndlm";
          version = "1.0";
          src = ./.;
          cargoHash = "sha256-8f71QI+TlqZh4Ogx4asjCx8r4dNx2vjF3juuWuFcDys=";

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
            programs.sway.enable = true;
            services.greetd = {
              enable = true;
              settings = {
                default_session = {
                  command = lib.mkForce "${ndlm}/bin/ndlm --session ${pkgs.sway}/bin/sway --theme-file ${(pkgs.catppuccin-plymouth.override { variant = "mocha"; })}/share/plymouth/themes/catppuccin-mocha/catppuccin-mocha.plymouth";
                  user = "greeter";
                };
              };
            };
            users.users = {
              test = {
                isNormalUser = true;
                password = "test";
              };
              greeter = {
                extraGroups = [
                  "video"
                  "input"
                  "render"
                ];
              };
            };
          }
        ];
      };
      packages.${system}.default = self.nixosConfigurations.default.config.system.build.vm;
    };
}
