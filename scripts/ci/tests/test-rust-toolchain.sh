#!/usr/bin/env bash
# Each case deliberately isolates environment changes in a subshell.
# shellcheck disable=SC2030,SC2031
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/repo/scripts/shared" "$fixture/custom cargo/bin" "$fixture/tools" "$fixture/hostile"
cp "$root/scripts/shared/rust-toolchain.sh" "$fixture/repo/scripts/shared/"
printf '[toolchain]\nchannel = "9.8.7"\n' >"$fixture/repo/rust-toolchain.toml"
cat >"$fixture/custom cargo/bin/rustup" <<'SH'
#!/usr/bin/env bash
[[ "$*" == 'which --toolchain 9.8.7 rustc' ]] || exit 1
printf '%s/tools/rustc\n' "$FIXTURE"
SH
for tool in cargo rustc rustdoc cargo-clippy clippy-driver cargo-fmt rustfmt; do
  printf '#!/usr/bin/env bash\nprintf "%%s 9.8.7 (fixture)\\n" "%s"\n' "$tool" >"$fixture/tools/$tool"
done
printf '#!/usr/bin/env bash\necho cargo 9.8.70\n' >"$fixture/hostile/cargo"
chmod +x "$fixture/custom cargo/bin/rustup" "$fixture/tools/"* "$fixture/hostile/cargo"
export FIXTURE="$fixture" CARGO_HOME="$fixture/custom cargo"
export PATH="$fixture/hostile:$PATH"
unset CARGO
# shellcheck source=scripts/shared/rust-toolchain.sh
source "$fixture/repo/scripts/shared/rust-toolchain.sh"
(
  cd /
  export RUSTUP_TOOLCHAIN=stable RUSTC=/missing
  require_pinned_toolchain
  [[ "$CARGO" == "$fixture/tools/cargo" && "$RUSTC" == "$fixture/tools/rustc" ]]
  [[ "$(command -v cargo-clippy)" == "$fixture/tools/cargo-clippy" ]]
  [[ "$(command -v cargo-fmt)" == "$fixture/tools/cargo-fmt" ]]
  cd "$fixture"
  [[ "$("$CARGO" --version)" == 'cargo 9.8.7 (fixture)' ]]
)
if (export CARGO="$fixture/hostile/cargo"; require_pinned_toolchain); then
  echo 'Accepted mismatched override' >&2; exit 1
fi
(export CARGO="$fixture/tools/cargo"; require_pinned_toolchain)
mv "$fixture/tools/rustc" "$fixture/tools/original"
cp "$fixture/hostile/cargo" "$fixture/tools/rustc"
if (require_pinned_toolchain); then exit 1; fi
mv "$fixture/tools/original" "$fixture/tools/rustc"
mv "$fixture/tools/cargo-clippy" "$fixture/tools/original"
if (require_pinned_toolchain); then exit 1; fi
mv "$fixture/tools/original" "$fixture/tools/cargo-clippy"
printf '[toolchain]\nchannel = "stable"\n' >"$fixture/repo/rust-toolchain.toml"
if (require_pinned_toolchain); then exit 1; fi
for content in \
  $'[toolchain]\nchannel = "9.8.7' \
  $'[toolchain]\nchannel = "9.8.7"\nchannel = "9.8.7"' \
  $'[other]\nchannel = "9.8.7"'; do
  printf '%s\n' "$content" >"$fixture/repo/rust-toolchain.toml"
  if (require_pinned_toolchain); then exit 1; fi
done
printf '[toolchain]\nchannel = "9.8.7"\n' >"$fixture/repo/rust-toolchain.toml"
if (export CARGO='missing-cargo --flag'; require_pinned_toolchain); then exit 1; fi
(
  unset CARGO_HOME
  export PATH="$fixture/custom cargo/bin:$PATH"
  require_pinned_toolchain
)
printf '[toolchain]\nchannel = "9.8.6"\n' >"$fixture/repo/rust-toolchain.toml"
if (require_pinned_toolchain); then exit 1; fi
echo 'Rust toolchain fixtures passed.'
