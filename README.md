# UpdateWP

Update a WordPress installation, its plugins, themes and translations step-by-step, optionally backing up the database before each step and making commits after each one.

## Requirements

* [Nix](https://nixos.org/download/)

or

* [Rust](https://www.rust-lang.org/tools/install)
* [Git](https://git-scm.com/downloads)
* [WP-CLI](https://wp-cli.org/)

## Setting up the Nix development environment

You can skip ahead to the "[Compiling the program](#compiling-the-program)" section if you already have Rust, Git and WP-CLI installed.

1. Navigate to the "devenv" directory.
```sh
cd devenv
```
2. Initialise the development environment. You can omit `--experimental-features "nix-command flakes"` if you've [enabled flakes globally](https://nixos.wiki/wiki/Flakes#Other_Distros.2C_without_Home-Manager).
```sh
nix develop --experimental-features "nix-command flakes" --impure
```

You should now have access to the required programs, such as Rust's `cargo`. Try running:

```sh
cargo -V
```

## Compiling the program

1. Navigate back to the project directory.
```sh
cd ..
```
2. Compile the program
```sh
cargo build -r
```

You can now run the program (even outside of the development environment, although you'll need Git and WP-CLI installed):

```sh
./target/release/update-wp -h
```
