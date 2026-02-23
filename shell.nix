{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    pkg-config
    openssl
    fuse3
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer
  ];

  shellHook = ''
    echo "rqbit-fuse development shell"
    echo "Run 'cargo build' to build the project"
    echo "Run 'cargo test' to run tests"
    echo "Run 'cargo clippy' to run linting"
    echo "Run 'cargo fmt' to format code"
  '';
}
