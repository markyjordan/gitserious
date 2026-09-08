#!/usr/bin/env bash
# Source after creating fake_bin. All tools remain inside the fixture directory.
# Caller supplies script_dir and fake_bin; generated scripts expand at runtime.
# shellcheck disable=SC2154,SC2016
fixture_pin="$(awk -F '"' '/^channel =/ {print $2}' "$script_dir/../../rust-toolchain.toml")"
export FIXTURE_RUST_PIN="$fixture_pin"
export FIXTURE_RUST_TOOLS="$fake_bin/toolchain"
export CARGO_HOME="$fake_bin/rust-home"
export CARGO="$fake_bin/cargo"
mkdir -p "$FIXTURE_RUST_TOOLS" "$CARGO_HOME/bin"
cat >"$CARGO_HOME/bin/rustup" <<'SH'
#!/usr/bin/env bash
[[ "$*" == "which --toolchain $FIXTURE_RUST_PIN rustc" ]] || exit 1
printf '%s/rustc\n' "$FIXTURE_RUST_TOOLS"
SH
for tool in cargo rustc rustdoc cargo-clippy clippy-driver cargo-fmt rustfmt; do
  printf '#!/usr/bin/env bash\nprintf "%%s %%s\\n" "%s" "$FIXTURE_RUST_PIN"\n' "$tool" >"$FIXTURE_RUST_TOOLS/$tool"
done
chmod +x "$CARGO_HOME/bin/rustup" "$FIXTURE_RUST_TOOLS/"*
