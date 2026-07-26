{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      rust-toolchain-toml = builtins.readFile ./rust-toolchain.toml;
      rust-toolchain = fromTOML rust-toolchain-toml;
    in
    {
      devShells = builtins.mapAttrs (
        system: rustPkgs:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs = [
              pkgs.just
              pkgs.rustup
              (
                rustPkgs."rust_${
                  nixpkgs.lib.replaceStrings [ "." ] [ "_" ] rust-toolchain.toolchain.channel
                }".override
                {
                  extensions = [
                    "rust-src"
                    "rust-analyzer"
                  ];
                }
              )
              pkgs.clang
              pkgs.pkg-config
              pkgs.temurin-bin-25
              pkgs.git
              pkgs.curl
              pkgs.jq
            ];

            shellHook = ''
              export PATH="$HOME/.cargo/bin:$PATH"
            '';
          };
        }
      ) rust-overlay.packages;

      formatter = builtins.mapAttrs (_: pkgs: pkgs.nixfmt-tree) nixpkgs.legacyPackages;
    };
}
