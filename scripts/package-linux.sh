#!/usr/bin/env bash
# Packages the desktop GUI and the CLI for Linux and tars them for a release.
#
# The binaries need the glibc of the build machine or newer, so build on the oldest distro to support.
# Writes `target/linux/N-Queens-Solver-<version>-linux-<arch>/` and prints the path of the tarball next to it.
set -euo pipefail

cd "$(dirname "$0")/.."

app_name="N-Queens Solver"
app_id="io.github.andreashdez.n-queens-solver"
gui="n_queens_gui"
cli="n_queens_problem"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
host=$(rustc -vV | sed -n 's/^host: //p')
out="target/linux"
name="N-Queens-Solver-$version-linux-${host%%-*}"
dir="$out/$name"
tarball="$out/$name.tar.gz"

cargo build --release --locked --features gui --bin "$gui" >&2
cargo build --release --locked --bin "$cli" >&2

rm -rf "$dir" "$tarball"
mkdir -p "$dir"
cp "target/release/$gui" "target/release/$cli" "$dir/"
cp assets/app-icon.png "$dir/$app_id.png"
cp LICENSE "$dir/"

# Without an app ID, winit names the X11 window class after the executable.
cat > "$dir/$app_id.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$app_name
Comment=Place N queens on an N×N board with a genetic algorithm
Exec=$gui
Icon=$app_id
Terminal=false
Categories=Education;Science;Math;
StartupWMClass=$gui
EOF

cat > "$dir/README.txt" <<EOF
$app_name $version for Linux

  ./$gui         opens the desktop GUI
  ./$cli     runs the command line solver (--help lists the options)

The GUI needs a desktop session with OpenGL. Exporting metrics uses the
desktop's file dialog through xdg-desktop-portal, or zenity if it is missing.

To add the GUI to your application menu, install it into your home folder:

  install -Dm755 -t ~/.local/bin $gui $cli
  install -Dm644 $app_id.png ~/.local/share/icons/hicolor/512x512/apps/$app_id.png
  install -Dm644 -t ~/.local/share/applications $app_id.desktop

The menu entry starts $gui from your PATH, so ~/.local/bin must be on it.

Documentation: https://andreashdez.github.io/n_queens_problem_rust/
EOF

tar -czf "$tarball" -C "$out" "$name"
echo "$tarball"
