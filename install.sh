#!/usr/bin/env bash
#
# install.sh — build and install `ghs` (github-status) so you can run it anywhere.
#
# Usage:
#   ./install.sh                 # build --release and install to a dir on your PATH
#   ./install.sh --prefix ~/bin  # install to a custom directory
#   ./install.sh --debug         # faster build, unoptimized
#   ./install.sh --no-setup      # skip the post-install setup wizard
#   ./install.sh --uninstall     # remove the installed binary
#
# Honors:
#   PREFIX=<dir>   same as --prefix
#
set -euo pipefail

BIN_NAME="ghs"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# --- pretty output -----------------------------------------------------------
if [[ -t 1 ]]; then
  C_RESET=$'\033[0m'; C_BLUE=$'\033[34m'; C_GREEN=$'\033[32m'
  C_YELLOW=$'\033[33m'; C_RED=$'\033[31m'; C_BOLD=$'\033[1m'; C_DIM=$'\033[2m'
else
  C_RESET=""; C_BLUE=""; C_GREEN=""; C_YELLOW=""; C_RED=""; C_BOLD=""; C_DIM=""
fi
info()  { printf '%s▸%s %s\n'  "$C_BLUE"   "$C_RESET" "$*"; }
ok()    { printf '%s✓%s %s\n'  "$C_GREEN"  "$C_RESET" "$*"; }
warn()  { printf '%s!%s %s\n'  "$C_YELLOW" "$C_RESET" "$*"; }
err()   { printf '%s✗%s %s\n'  "$C_RED"    "$C_RESET" "$*" >&2; }
die()   { err "$@"; exit 1; }

# --- args --------------------------------------------------------------------
PROFILE="release"
PREFIX="${PREFIX:-}"
UNINSTALL=0
DO_SETUP=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) PREFIX="${2:-}"; shift 2 ;;
    --prefix=*) PREFIX="${1#*=}"; shift ;;
    --debug) PROFILE="debug"; shift ;;
    --release) PROFILE="release"; shift ;;
    --no-setup) DO_SETUP=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help)
      sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
done

# --- pick an install dir on PATH ---------------------------------------------
# Preference order when --prefix not given: ~/.local/bin, ~/.cargo/bin,
# /usr/local/bin. We pick the first that's already on $PATH (and writable, or
# writable via sudo for /usr/local/bin).
choose_prefix() {
  local candidates=("$HOME/.local/bin" "$HOME/.cargo/bin" "/usr/local/bin")
  local on_path=()
  local d
  for d in "${candidates[@]}"; do
    case ":$PATH:" in *":$d:"*) on_path+=("$d") ;; esac
  done
  # Prefer dirs already on PATH; fall back to ~/.local/bin.
  for d in "${on_path[@]}"; do
    if [[ -w "$d" || ( ! -e "$d" && -w "$(dirname "$d")" ) ]]; then
      printf '%s' "$d"; return 0
    fi
  done
  # Nothing writable on PATH — default to ~/.local/bin (we'll create it).
  printf '%s' "$HOME/.local/bin"
}

[[ -n "$PREFIX" ]] || PREFIX="$(choose_prefix)"
TARGET="$PREFIX/$BIN_NAME"

# --- uninstall ---------------------------------------------------------------
if [[ "$UNINSTALL" -eq 1 ]]; then
  # Remove from the chosen prefix and any other known dir, for good measure.
  removed=0
  for d in "$PREFIX" "$HOME/.local/bin" "$HOME/.cargo/bin" "/usr/local/bin"; do
    f="$d/$BIN_NAME"
    if [[ -e "$f" ]]; then
      if [[ -w "$d" ]]; then rm -f "$f"; else sudo rm -f "$f"; fi
      ok "removed $f"; removed=1
    fi
  done
  [[ "$removed" -eq 1 ]] || warn "no installed '$BIN_NAME' found"
  exit 0
fi

# --- prerequisites -----------------------------------------------------------
command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust: https://rustup.rs"

printf '%s%s github-status installer %s\n\n' "$C_BOLD" "$C_BLUE" "$C_RESET"
info "Project : $SCRIPT_DIR"
info "Profile : $PROFILE"
info "Install : $TARGET"
echo

# --- build -------------------------------------------------------------------
info "Building (cargo build $([[ $PROFILE == release ]] && echo --release)) …"
cd "$SCRIPT_DIR"
if [[ "$PROFILE" == "release" ]]; then
  cargo build --release
  BUILT="$SCRIPT_DIR/target/release/$BIN_NAME"
else
  cargo build
  BUILT="$SCRIPT_DIR/target/debug/$BIN_NAME"
fi
[[ -f "$BUILT" ]] || die "build did not produce $BUILT"
ok "Built $BUILT"

# --- install -----------------------------------------------------------------
mkdir -p "$PREFIX" 2>/dev/null || sudo mkdir -p "$PREFIX"
if [[ -w "$PREFIX" ]]; then
  install -m 0755 "$BUILT" "$TARGET"
else
  warn "$PREFIX is not writable — using sudo"
  sudo install -m 0755 "$BUILT" "$TARGET"
fi
ok "Installed → $TARGET"

# --- PATH check --------------------------------------------------------------
echo
case ":$PATH:" in
  *":$PREFIX:"*)
    ok "$PREFIX is on your PATH"
    ;;
  *)
    warn "$PREFIX is NOT on your PATH."
    shell_rc="$HOME/.bashrc"
    [[ "${SHELL:-}" == *zsh* ]] && shell_rc="$HOME/.zshrc"
    printf '  Add this line to %s%s%s:\n' "$C_BOLD" "$shell_rc" "$C_RESET"
    printf '    %sexport PATH="%s:$PATH"%s\n' "$C_DIM" "$PREFIX" "$C_RESET"
    ;;
esac

# --- post-install setup wizard ----------------------------------------------
# Make the very first run effortless: detect auth, then offer to populate the
# config automatically. Skipped with --no-setup or when stdin isn't a terminal.
run_setup() {
  local ghs_bin="$TARGET"
  command -v "$BIN_NAME" >/dev/null 2>&1 && ghs_bin="$BIN_NAME"

  echo
  printf '%s%s Setup %s\n' "$C_BOLD" "$C_BLUE" "$C_RESET"
  info "Running diagnostics …"
  echo
  "$ghs_bin" doctor || true

  # If a config already has projects, don't nag.
  if "$ghs_bin" list >/dev/null 2>&1 && [[ -n "$("$ghs_bin" list 2>/dev/null | grep -vE 'No projects|ghs add|ghs import')" ]]; then
    return 0
  fi

  echo
  printf 'How would you like to add repositories?\n'
  printf '  %s1%s) Scan this folder'\''s git clones   (ghs import --local)\n' "$C_BOLD" "$C_RESET"
  printf '  %s2%s) Import your GitHub repos         (ghs import --me)\n'  "$C_BOLD" "$C_RESET"
  printf '  %s3%s) Start with two examples          (ghs init)\n'         "$C_BOLD" "$C_RESET"
  printf '  %s4%s) Skip — I'\''ll add them later\n'                        "$C_BOLD" "$C_RESET"
  printf 'Choice [1-4]: '
  local choice; read -r choice || choice=4

  case "$choice" in
    1) echo; "$ghs_bin" import --local || true ;;
    2) echo; "$ghs_bin" import --me || true ;;
    3) echo; "$ghs_bin" init --force || true ;;
    *) info "No problem — add repos anytime with: ghs add owner/repo" ;;
  esac
}

if [[ "$DO_SETUP" -eq 1 && -t 0 ]]; then
  run_setup
fi

# --- done --------------------------------------------------------------------
echo
ok "Done! Try it:"
printf '    %sghs%s               # launch the dashboard\n' "$C_BOLD" "$C_RESET"
printf '    %sghs add%s owner/repo # track a repo\n' "$C_BOLD" "$C_RESET"
printf '    %sghs import%s --me    # import your GitHub repos\n' "$C_BOLD" "$C_RESET"
printf '    %sghs doctor%s         # re-check your setup\n' "$C_BOLD" "$C_RESET"
echo
printf '  %sTip:%s auth is auto-detected from `gh` or GITHUB_TOKEN — no setup needed.\n' "$C_DIM" "$C_RESET"
