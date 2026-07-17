pkgs:
let
  rustInputs = with pkgs; [ clang libiconv pkg-config ];

  rustEnv = ''
    export LIBRARY_PATH="${pkgs.libiconv}/lib''${LIBRARY_PATH:+:$LIBRARY_PATH}"
    export NIX_LDFLAGS="-L${pkgs.libiconv}/lib''${NIX_LDFLAGS:+ $NIX_LDFLAGS}"
    export NIX_CFLAGS_COMPILE="-I${pkgs.libiconv}/include''${NIX_CFLAGS_COMPILE:+ $NIX_CFLAGS_COMPILE}"
  '';

  mkRust = { extraInputs, script }: {
    inputs = rustInputs ++ extraInputs;
    script = rustEnv + script;
  };
in
{
  ci = mkRust {
    extraInputs = [ pkgs.git pkgs.jdk25 pkgs.typos ];
    script = ''
      typos
      export JAVA_HOME=${pkgs.jdk25.home}
      cargo ck
      cargo test --workspace --all-features
      cargo lint -- -D warnings
      git diff --exit-code
    '';
  };

  ready = mkRust {
    extraInputs = [ pkgs.git pkgs.jdk25 pkgs.typos ];
    script = ''
      git diff --exit-code --quiet
      typos
      export JAVA_HOME=${pkgs.jdk25.home}
      cargo fmt
      cargo ck
      cargo test --all-features
      cargo lint -- -D warnings
      RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items
      git status
    '';
  };

  fix = mkRust {
    extraInputs = [ pkgs.git pkgs.typos ];
    script = ''
      cargo clippy --fix --allow-staged --no-deps
      cargo fmt
      typos -w
      git status
    '';
  };

  install-hook.script = ''
    echo -e "#/bin/sh\nnix run .#fmt" > .git/hooks/pre-commit
    chmod +x .git/hooks/pre-commit
  '';

  fmt.script = ''
    cargo fmt
  '';

  bump-version = mkRust {
    extraInputs = [ pkgs.curl pkgs.jdk25 pkgs.jq ];
    script = ''
      export JAVA_HOME=${pkgs.jdk25.home}
      curl -s https://piston-meta.mojang.com/mc/game/version_manifest_v2.json | jq -r '.versions.[0].id' > mc-version
      if ! git diff --quiet -- mc-version; then
        rm -rf target
        cargo r -r -p data_gen
        rm -rf crates/mcre_world/src/data
        cargo r -r -p world_data_gen
        cargo fmt
      fi
    '';
  };
}
