{
  inputs = { 
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url ="github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, fenix }: 
    flake-utils.lib.eachDefaultSystem 
      (system:
        let 
          overlays = [ ( import fenix ) ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };
        in
        with pkgs;
        {
          devShells.native = mkShell {
            buildInputs = [ 
              rust-bin.stable.latest.default
            ];
          };

          devShells.wasm = mkShell {
            buildInputs = [
              rust-bin.stable.latest.default
              wasm-pack
            ];
          };

          #devShells.default = devShells.native;
        }
      );
}
