#!/usr/bin/env bash
# Packages the desktop GUI and the CLI for Windows and zips them for a release.
#
# Needs Windows with the MSVC Rust toolchain, Git Bash and 7-Zip, as on GitHub's Windows runners.
# Writes `target/windows/N-Queens-Solver-<version>-windows-<arch>/` and prints the path of the zip next to it.
set -euo pipefail

cd "$(dirname "$0")/.."

app_name="N-Queens Solver"
gui="n_queens_gui"
cli="n_queens_problem"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
host=$(rustc -vV | sed -n 's/^host: //p')
out="target/windows"
name="N-Queens-Solver-$version-windows-${host%%-*}"
dir="$out/$name"
zip="$out/$name.zip"

# A static C runtime lets the executables run without the Visual C++ Redistributable.
# The explicit target keeps the flag away from build scripts and proc macros.
export RUSTFLAGS="-C target-feature=+crt-static"
cargo build --release --locked --features gui --bin "$gui" --target "$host" >&2
cargo build --release --locked --bin "$cli" --target "$host" >&2

rm -rf "$dir" "$zip"
mkdir -p "$dir"
cp "target/$host/release/$gui.exe" "target/$host/release/$cli.exe" "$dir/"
cp LICENSE "$dir/LICENSE.txt"

cat > "$dir/README.txt" <<EOF
$app_name $version for Windows

  $gui.exe         opens the desktop GUI
  $cli.exe     runs the command line solver (--help lists the options)

The executables are not signed, so Windows SmartScreen may block the first
launch. Click "More info", then "Run anyway".

Documentation: https://andreashdez.github.io/n_queens_problem_rust/
EOF

(cd "$out" && 7z a -tzip "$name.zip" "$name" >&2)
echo "$zip"
