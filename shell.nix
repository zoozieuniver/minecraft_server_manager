{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    rustc
    cargo
    pkg-config
    cmake
  ];
  buildInputs = with pkgs; [
    gtk3
    glib
    gdk-pixbuf
    cairo
    pango
    atk
    wayland
    openssl
    libxkbcommon
    libX11
    libxcursor
    libxrandr
    libxi
    vulkan-loader
  ];
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    gtk3
    glib
    gdk-pixbuf
    cairo
    pango
    atk
    wayland
    libxkbcommon
    libX11
    libxcursor
    libxrandr
    libxi
    vulkan-loader
    libGL
  ]);
}
