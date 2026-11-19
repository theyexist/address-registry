{
  description = "A machine-readable registry of Cardano scripts, keyed by payment credential";

  # Two public inputs, the same two `agent-playbook` pins: nixpkgs, and fenix for the
  # toolchain. Nothing private is an input and nothing is inherited from the org flake,
  # because this repository is meant to be built by someone outside it — the flake is the
  # whole toolchain story. The pin is *matched* to the org's rather than inherited, so a
  # bump in both places leaves every repository on one rustc.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, fenix, ... }:
    let
      lib = nixpkgs.lib;
      # `x86_64-darwin` is absent on purpose: the pinned nixpkgs (26.11) dropped it, and
      # listing a system the input cannot evaluate makes `nix flake check --all-systems`
      # fail for everyone. An Intel Mac builds this from the 26.05-darwin branch instead.
      systems = [
        "aarch64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = lib.genAttrs systems;
      pkgsFor = system: import nixpkgs { inherit system; };

      rustToolchainFor =
        system:
        let
          fenixPkgs = fenix.packages.${system};
        in
        fenixPkgs.combine [
          fenixPkgs.stable.cargo
          fenixPkgs.stable.clippy
          fenixPkgs.stable.rustc
          fenixPkgs.stable.rustfmt
        ];

      # `makeRustPlatform` wants cargo and rustc separately; a combined toolchain carries
      # both, so it is passed twice.
      rustPlatformFor =
        pkgs:
        let
          toolchain = rustToolchainFor pkgs.stdenv.hostPlatform.system;
        in
        pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

      # `vendor/` holds the two upstream clones and `target/` the build directory; neither
      # is an input to the package. A tracked repository would have the flake source drop
      # them via `.gitignore`, but filtering here keeps `nix build` honest before the first
      # commit and when someone points `path:` at a working copy.
      source = lib.cleanSourceWith {
        src = ./.;
        filter =
          path: _type:
          !(builtins.elem (baseNameOf (toString path)) [
            "vendor"
            "target"
            ".direnv"
            ".tmp"
          ]);
      };

      # The importer. `cargoLock` rather than a hash: `Cargo.lock` is committed, so a
      # dependency added without re-locking fails here rather than in a consumer's build.
      registryFor =
        rustPlatform:
        rustPlatform.buildRustPackage {
          pname = "address-registry";
          version = (lib.importTOML ./Cargo.toml).package.version;
          src = source;
          cargoLock.lockFile = ./Cargo.lock;
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          rustPlatform = rustPlatformFor (pkgsFor system);
        in
        {
          default = registryFor rustPlatform;
        }
      );

      # `nix develop` here, so the repository follows `devshell-first` without a second
      # toolchain appearing on anyone's PATH. No wasm targets: nothing here builds for wasm.
      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            packages = [
              (rustToolchainFor system)
              # Not for building: the importer records each upstream's revision by running
              # `git -C <clone> rev-parse HEAD`, so an import inside the shell needs git.
              pkgs.git
            ];
          };
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          rustPlatform = rustPlatformFor pkgs;
          registry = registryFor rustPlatform;
        in
        {
          # Through `buildRustPackage` rather than a bare `runCommand` with the toolchain
          # on PATH: the crate has real dependencies, and only this gives them a registry
          # to resolve from up front.
          tests = rustPlatform.buildRustPackage {
            pname = "address-registry-tests";
            version = (lib.importTOML ./Cargo.toml).package.version;
            src = source;
            cargoLock.lockFile = ./Cargo.lock;
            buildPhase = "cargo build --offline";
            doCheck = true;
            checkPhase = "cargo test --offline";
            installPhase = "touch $out";
          };

          # The binary answers its own usage line, and names both commands. Cheap,
          # and it is the one thing a unit test cannot cover: that the installed
          # executable is the importer.
          cli = pkgs.runCommand "address-registry-cli" { } ''
            ${registry}/bin/address-registry --help | grep -q "usage: address-registry"
            ${registry}/bin/address-registry --help | grep -q "resolve"
            touch $out
          '';
        }
      );
    };
}
