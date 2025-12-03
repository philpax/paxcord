{ pkgs ? import <nixpkgs> { } }:

with pkgs;

mkShell rec {
  nativeBuildInputs = [
    pkg-config
    openssl
  ];
  buildInputs = [];
  LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
}