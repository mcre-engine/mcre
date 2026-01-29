pkgs: {
  ci = {
    inputs = [
      pkgs.git
      pkgs.jdk25
      pkgs.typos
    ];
    script = ''
      typos
      export JAVA_HOME=${pkgs.jdk25.home}
      cargo ck
      cargo test --workspace --all-features
      cargo lint -- -D warnings
      git diff --exit-code
    '';
  };

  ready = {
    inputs = [
      pkgs.git
      pkgs.typos
    ];
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

  fix = {
    inputs = [
      pkgs.git
      pkgs.typos
    ];
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

  bump-version = {
    inputs = [pkgs.jq pkgs.curl pkgs.jdk25];
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
