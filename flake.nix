{
  description = "veric — aski program verifier: per-module rkyv → verified program rkyv";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    veri-core = {
      url = "github:LiGoldragon/veri-core";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
      inputs.crane.follows = "crane";
      inputs.flake-utils.follows = "flake-utils";
    };
    askicc = {
      url = "github:LiGoldragon/askicc";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
      inputs.crane.follows = "crane";
      inputs.flake-utils.follows = "flake-utils";
    };
    askic = {
      url = "github:LiGoldragon/askic";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
      inputs.crane.follows = "crane";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = { self, nixpkgs, fenix, crane, flake-utils, veri-core, askicc, askic, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        toolchain = fenix.packages.${system}.stable.toolchain;
        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        veri-core-source = veri-core.packages.${system}.source;
        askicc-bin = askicc.packages.${system}.askicc;
        askic-bin = askic.packages.${system}.askic;
        dsls-data = askicc.packages.${system}.dsls-data;

        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            (craneLib.filterCargoSources path type)
            || (builtins.match ".*\\.aski$" path != null);
        };

        commonArgs = {
          inherit src;
          pname = "veric";
          version = "0.1.0";
          postUnpack = ''
            mkdir -p $sourceRoot/flake-crates
            cp -r ${veri-core-source} $sourceRoot/flake-crates/veri-core
            chmod -R +w $sourceRoot/flake-crates
          '';
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        veric = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });

        # Integration test: .aski → askic → veric
        integration-test = pkgs.runCommand "veric-integration-test" {
          nativeBuildInputs = [ askic-bin veric ];
          DIALECT_DATA = "${dsls-data}/dsls.rkyv";
        } ''
          mkdir -p $out work

          echo "=== Test 1: single module ==="
          askic ${./tests/single/simple.aski} work/simple.rkyv
          veric work/simple.rkyv -o work/simple-program.rkyv
          test -s work/simple-program.rkyv
          echo "PASS: single module"

          echo "=== Test 2: multi-module with valid imports ==="
          askic ${./tests/multi/valid/core.aski} work/core.rkyv
          askic ${./tests/multi/valid/app.aski} work/app.rkyv
          veric work/core.rkyv work/app.rkyv -o work/multi-program.rkyv
          test -s work/multi-program.rkyv
          echo "PASS: multi-module valid imports"

          echo "=== Test 3: circular import (must fail) ==="
          askic ${./tests/multi/errors/circular_a.aski} work/circular_a.rkyv
          askic ${./tests/multi/errors/circular_b.aski} work/circular_b.rkyv
          if veric work/circular_a.rkyv work/circular_b.rkyv -o work/circular-program.rkyv 2>work/err.txt; then
            echo "FAIL: circular import should have been rejected"
            exit 1
          fi
          grep -q "circular" work/err.txt
          echo "PASS: circular import detected"

          echo "=== Test 4: missing import module (must fail) ==="
          askic ${./tests/multi/errors/missing_import.aski} work/missing.rkyv
          if veric work/missing.rkyv -o work/missing-program.rkyv 2>work/err2.txt; then
            echo "FAIL: missing import should have been rejected"
            exit 1
          fi
          grep -q "does not exist" work/err2.txt
          echo "PASS: missing import detected"

          echo "ALL INTEGRATION TESTS PASSED"
          echo "4 tests passed" > $out/result.txt
        '';

      in {
        packages = {
          default = veric;
          veric = veric;
        };

        checks = {
          build = veric;
          unit-tests = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
          });
          integration = integration-test;
        };

        devShells.default = craneLib.devShell {
          packages = [ pkgs.rust-analyzer ];
        };
      }
    );
}
