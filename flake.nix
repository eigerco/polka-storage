{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        # This is not pretty. I couldn't make it work with nix flake `github:paritytech/zombienet`.
        zombienet = pkgs.stdenv.mkDerivation rec {
          name = "zombienet";
          pname = name;
          src = builtins.fetchurl {
            url = "https://github.com/paritytech/zombienet/releases/download/v1.3.103/zombienet-linux-x64";
            sha256 = "sha256:1qlsvd3h4szcgzj2990qgig6vcrg5grzfxkzhdhg93378fmlz9lx";
          };
          phases = [ "installPhase" ];
          installPhase = ''
            mkdir -p $out/bin
            cp $src $out/bin/${name}
            chmod +x $out/bin/${name}
          '';
        };
        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        # build-time
        nativeBuildInputs = with pkgs; [ pkg-config rustToolchain ];
        # runtime
        buildInputs = with pkgs; [ clang polkadot zombienet ] ++ lib.optionals stdenv.isDarwin [
          darwin.apple_sdk.frameworks.Security
        ];
      in
      with pkgs;
      {
        devShells.default = mkShell {
          inherit buildInputs nativeBuildInputs;

          LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";
          PROTOC = "${protobuf}/bin/protoc";
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library/";
          ROCKSDB_LIB_DIR = "${rocksdb}/lib";
        };
      }
    );
}
