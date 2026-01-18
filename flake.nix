{
  description = "MCRE Dev shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      systems =
        [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      rust-toolchain-toml = builtins.readFile ./rust-toolchain.toml;
      rust-toolchain = builtins.fromTOML rust-toolchain-toml;
    in {
      devShells = nixpkgs.lib.genAttrs systems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
        in {
          default = pkgs.mkShell {
            buildInputs = [
              pkgs.just
              pkgs.rustup
              pkgs.rust-bin.stable."${rust-toolchain.toolchain.channel}".default

              pkgs.clang
              pkgs.pkg-config
              pkgs.jdk25
              pkgs.git
            ];

            shellHook = ''
              export PATH="$HOME/.cargo/bin:$PATH"
            '';
          };
        });
    };
}

