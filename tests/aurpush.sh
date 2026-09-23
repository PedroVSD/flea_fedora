#!/usr/bin/env bash
# packaging/aur-push pushes only flea, flea-bin and flea-git, because its key reaches every package the AUR account maintains.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
checks=0
failed=0
check() {
    checks=$((checks + 1))
    if [ "$2" = "$3" ]; then printf 'ok   %s\n' "$1"; else printf 'FAIL %s: expected [%s] got [%s]\n' "$1" "$2" "$3"; failed=$((failed + 1)); fi
}
box=$(mktemp -d "${TMPDIR:-/tmp}/flea-aurpush.XXXXXX") || exit 1
trap 'rm -rf -- "$box"' EXIT
printf 'pkgname=flea\npkgver=0.0.1\npkgrel=1\narch=(any)\n' > "$box/PKGBUILD"
export AUR_COMMIT_NAME='Flea test' AUR_COMMIT_EMAIL='flea-test@example.invalid' AUR_REMOTE_BASE="file://$box/remote"

for name in other-package flea- 'flea bin' ../flea FLEA flea-git.git; do
    out=$(packaging/aur-push "$name" "$box/PKGBUILD" 'test' 2>&1)
    status=$?
    check "'$name' is refused before any clone" "2 aur-push: refusing '$name', which is not flea, flea-bin or flea-git" "$status $out"
done

# The allowed name goes on to its own bare repository and nothing else; makepkg writes the .SRCINFO the AUR requires.
if command -v makepkg >/dev/null; then
    mkdir -p "$box/remote"
    git init -q --bare -b master "$box/remote/flea.git"
    git init -q -b master "$box/seed" && git -C "$box/seed" -c user.name=seed -c user.email=seed@example.invalid commit -q --allow-empty -m seed \
        && git -C "$box/seed" push -q "$box/remote/flea.git" master
    packaging/aur-push flea "$box/PKGBUILD" 'test: flea' > "$box/push.log" 2>&1
    check "flea is pushed to its own repository" "0" "$?"
    check "the pushed commit carries the given author" "Flea test <flea-test@example.invalid>" "$(git -C "$box/remote/flea.git" log -1 --format='%an <%ae>' master)"
else
    printf 'SKIP the allowed-name push: makepkg is not installed here, the box runs it\n'
fi
printf 'aurpush: %d check(s), %d failed\n' "$checks" "$failed"
[ "$failed" -eq 0 ]
