#!/usr/bin/env bash
# Packages the desktop GUI and the CLI for Linux and tars them for a release.
#
# The binaries need the glibc of the build machine or newer, so build on the oldest distro to support.
# Writes `target/linux/N-Queens-Solver-<version>-linux-<arch>/` and prints the path of the tarball next to it.
set -euo pipefail

cd "$(dirname "$0")/.."

app_name="N-Queens Solver"
app_id="io.github.andreashdez.n-queens-solver"
cli="n_queens_problem"

# GUI=slint packages the Slint GUI, which is replacing the egui one (the default).
case "${GUI:-egui}" in
    # Without an app ID, winit names the X11 window class after the executable.
    egui) gui="n_queens_gui" features="gui" wm_class="n_queens_gui" needs="OpenGL" ;;
    # The Slint GUI sets the app ID as its X11 window class and Wayland app ID.
    slint) gui="n_queens_slint" features="gui-slint" wm_class="$app_id" needs="OpenGL and fontconfig" ;;
    *) echo "GUI must be egui or slint, not ${GUI}" >&2; exit 1 ;;
esac

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
host=$(rustc -vV | sed -n 's/^host: //p')
out="target/linux"
name="N-Queens-Solver-$version-linux-${host%%-*}"
dir="$out/$name"
tarball="$out/$name.tar.gz"

cargo build --release --locked --features "$features" --bin "$gui" >&2
cargo build --release --locked --bin "$cli" >&2

rm -rf "$dir" "$tarball"
mkdir -p "$dir"
cp "target/release/$gui" "target/release/$cli" "$dir/"
cp assets/app-icon.png "$dir/$app_id.png"
cp LICENSE "$dir/"

cat > "$dir/$app_id.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$app_name
Comment=Place N queens on an N×N board with a genetic algorithm
Exec=$gui
Icon=$app_id
Terminal=false
Categories=Education;Science;Math;
StartupWMClass=$wm_class
EOF

cat > "$dir/README.txt" <<EOF
$app_name $version for Linux

  ./$gui         opens the desktop GUI
  ./$cli     runs the command line solver (--help lists the options)

The GUI needs a desktop session with $needs. Exporting metrics uses the
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
