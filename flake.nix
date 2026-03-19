{
  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

  outputs = { nixpkgs, ... }:
    let pkgs = nixpkgs.legacyPackages.x86_64-linux;
    in {
      devShells.x86_64-linux.default = pkgs.mkShell {
        packages = with pkgs; [
          # Frontend (React + Vite + TypeScript)
          nodejs

          # Rust backend
          rustc
          cargo
          pkg-config
          llvmPackages.clang
          llvmPackages.libclang

          # Runtime deps + build deps (dynamic linking in dev)
          ffmpeg-headless
          ffmpeg-headless.dev
          lame
          bzip2
          openapv # workaround: nixpkgs emits broken -L path for openapv, adding it here fixes LIBRARY_PATH
        ];

        env = {
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        };
      };
    };
}
