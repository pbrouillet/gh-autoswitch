#!/usr/bin/env bash
#
# Dependency-free test runner for gh-autoswitch (bash credential helper).
# Uses a mock `gh` on PATH so no real GitHub auth is touched.
#
#   bash test/run.sh
#
set -uo pipefail

HERE="$(cd "$(dirname "$0")" >/dev/null 2>&1 && pwd)"
GHAS="$HERE/../bin/gh-autoswitch"

PASS=0
FAIL=0

ok()   { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; }

assert_eq() { # desc expected actual
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (expected [$2], got [$3])"; fi
}
assert_contains() { # desc haystack needle
  case "$2" in *"$3"*) ok "$1";; *) bad "$1 (missing [$3] in [$2])";; esac
}

# --- test workspace ------------------------------------------------------
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

export GH_CONFIG_DIR="$WORK/gh"
export GH_AUTOSWITCH_CONFIG="$WORK/config"
export MOCK_LOG="$WORK/gh.log"
mkdir -p "$GH_CONFIG_DIR" "$WORK/bin"

# Config file under test
cat > "$GH_AUTOSWITCH_CONFIG" <<'CFG'
# comment
github.com/acme-corp = alice_work
github.com/*         = alice_personal
CFG

write_hosts() { # active-user
  cat > "$GH_CONFIG_DIR/hosts.yml" <<EOF
github.com:
    git_protocol: https
    users:
        alice_work:
        alice_personal:
    user: $1
EOF
}

# Mock gh: logs switch calls, rewrites hosts.yml on switch, emits a fake
# credential on `git-credential get`.
cat > "$WORK/bin/gh" <<'MOCK'
#!/usr/bin/env bash
if [ "$1" = "auth" ] && [ "$2" = "switch" ]; then
  shift 2; host=""; user=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --hostname) host="$2"; shift 2;;
      --user) user="$2"; shift 2;;
      *) shift;;
    esac
  done
  echo "switch $host $user" >> "$MOCK_LOG"
  cat > "$GH_CONFIG_DIR/hosts.yml" <<EOF
$host:
    git_protocol: https
    users:
        alice_work:
        alice_personal:
    user: $user
EOF
  exit 0
fi
if [ "$1" = "auth" ] && [ "$2" = "git-credential" ]; then
  op="$3"; body="$(cat)"
  echo "gitcred $op" >> "$MOCK_LOG"
  if [ "$op" = "get" ]; then
    printf 'protocol=https\nhost=github.com\nusername=x-access-token\npassword=TOKEN\n'
  fi
  exit 0
fi
echo "unexpected gh args: $*" >&2
exit 99
MOCK
chmod +x "$WORK/bin/gh"
export PATH="$WORK/bin:$PATH"

run_get() { # owner  -> runs helper 'get' with a path for owner
  printf 'protocol=https\nhost=github.com\npath=%s/repo.git\n\n' "$1" \
    | bash "$GHAS" git-credential get
}

echo "gh-autoswitch tests"

# 1) exact match switches when not already active
write_hosts alice_personal; : > "$MOCK_LOG"
out="$(run_get acme-corp)"
assert_contains "exact: delegates and returns token" "$out" "password=TOKEN"
assert_contains "exact: switched to alice_work" "$(cat "$MOCK_LOG")" "switch github.com alice_work"

# 2) already-active: no switch call
write_hosts alice_work; : > "$MOCK_LOG"
run_get acme-corp >/dev/null
if grep -q '^switch ' "$MOCK_LOG"; then bad "already-active: must not switch"; else ok "already-active: no switch"; fi
assert_contains "already-active: still delegates" "$(cat "$MOCK_LOG")" "gitcred get"

# 3) wildcard fallback for unknown owner
write_hosts alice_work; : > "$MOCK_LOG"
run_get someoneelse >/dev/null
assert_contains "wildcard: switched to alice_personal" "$(cat "$MOCK_LOG")" "switch github.com alice_personal"

# 4) no matching host -> no switch, still delegates
write_hosts alice_work; : > "$MOCK_LOG"
printf 'protocol=https\nhost=other.example.com\npath=x/y.git\n\n' | bash "$GHAS" git-credential get >/dev/null
if grep -q '^switch ' "$MOCK_LOG"; then bad "no-match: must not switch"; else ok "no-match: no switch"; fi

# 5) store passes through without switching
write_hosts alice_personal; : > "$MOCK_LOG"
printf 'protocol=https\nhost=github.com\npath=acme-corp/repo.git\nusername=u\npassword=p\n\n' \
  | bash "$GHAS" git-credential store >/dev/null
if grep -q '^switch ' "$MOCK_LOG"; then bad "store: must not switch"; else ok "store: no switch"; fi
assert_contains "store: delegates to gh" "$(cat "$MOCK_LOG")" "gitcred store"

echo
echo "PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ]
