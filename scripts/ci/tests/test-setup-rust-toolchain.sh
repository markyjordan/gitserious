#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/repo/scripts/shared" "$fixture/custom cargo/bin"
cp "$root/scripts/shared/"*.sh "$fixture/repo/scripts/shared/"
export CARGO_HOME="$fixture/custom cargo" FIXTURE="$fixture"
unset CARGO
cat >"$CARGO_HOME/bin/rustup" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE/log"
case "$1 $2" in
  'toolchain install')
    [[ "$*" == "toolchain install $EXPECTED_PIN --profile minimal --component clippy,rustfmt --no-self-update" ]]
    [[ "${FAIL_INSTALL:-0}" == 0 ]] || exit 23
    mkdir -p "$FIXTURE/tools"
    for tool in cargo rustc rustdoc cargo-clippy clippy-driver cargo-fmt rustfmt; do
      printf '#!/usr/bin/env bash\necho "%s %s"\n' "$tool" "$EXPECTED_PIN" >"$FIXTURE/tools/$tool"
    done
    chmod +x "$FIXTURE/tools/"*
    ;;
  'target add')
    [[ "$*" == "target add --toolchain $EXPECTED_PIN x86_64-unknown-linux-gnu" ]]
    [[ "${FAIL_TARGET:-0}" == 0 ]] || exit 24
    ;;
  'which --toolchain')
    [[ "$*" == "which --toolchain $EXPECTED_PIN rustc" ]]
    [[ "${FAIL_VALIDATE:-0}" == 0 ]] || exit 25
    printf '%s/tools/rustc\n' "$FIXTURE"
    ;;
  *) exit 26 ;;
esac
SH
chmod +x "$CARGO_HOME/bin/rustup"
setup="$fixture/repo/scripts/shared/setup-rust-toolchain.sh"
for pin in 9.8.7 9.8.8; do
  export EXPECTED_PIN="$pin"
  printf '[toolchain]\nchannel = "%s"\n' "$pin" >"$fixture/repo/rust-toolchain.toml"
  : >"$fixture/log"
  (cd /; bash "$setup") >"$fixture/output"
  [[ "$(wc -l <"$fixture/log" | tr -d ' ')" == 2 ]]
  grep -Fx "cargo $pin" "$fixture/output"
  grep -Fx "rustc $pin" "$fixture/output"
  : >"$fixture/log"
  bash "$setup" --target x86_64-unknown-linux-gnu >/dev/null
  [[ "$(wc -l <"$fixture/log" | tr -d ' ')" == 3 ]]
done
for failure in FAIL_INSTALL FAIL_TARGET FAIL_VALIDATE; do
  if env "$failure=1" bash "$setup" --target x86_64-unknown-linux-gnu >/dev/null 2>&1; then
    echo "Ignored $failure" >&2; exit 1
  fi
done
for argument in --unknown --target; do
  : >"$fixture/log"
  if bash "$setup" "$argument" >/dev/null 2>&1; then exit 1; fi
  [[ ! -s "$fixture/log" ]]
done
: >"$fixture/log"
if bash "$setup" --target '--bad' >/dev/null 2>&1; then exit 1; fi
[[ ! -s "$fixture/log" ]]
echo 'Explicit toolchain setup fixtures passed.'
