# Defines the native Make Flea the default case; tests/ui.sh supplies the guarded fixture and IPC helpers.

# The row and the line under it as on|inert|note|role, the last two empty when off says nothing.
makedefault_state() {
    ipc settingsModel | jq -r '(map(.id) | index("makeDefault")) as $at
        | .[$at] as $row | (.[$at + 1] | if .kind == "hint" then . else {} end) as $note
        | [$row.on, ($row.inert // false), ($note.label // ""), ($note.role // "")] | map(tostring) | join("|")'
}

# The File manager fact, which states the same xdg-mime answer the box is ticked by.
makedefault_handler() {
    ipc settingsModel | jq -r '.[] | select(.label == "File manager") | .value'
}

makedefault_ran() {
    tr '\n' ';' < "$makedefault_log"
}

# How many portal restarts the stub swallowed, once every one of them is the switch's exact try-restart.
makedefault_restarts() {
    local log="$makedefault_restart_log"
    ! grep -q -v -x -F 'SYSTEMCTL --user try-restart xdg-desktop-portal.service' "$log" \
        || fail "makedefault: systemctl was asked something else about the portal: $(tr '\n' ';' < "$log")"
    grep -c . "$log" || true
}

makedefault_wait() {
    local want="$1" what="$2" got=""
    for _attempt in $(seq 1 100); do
        got=$(makedefault_state)
        [[ "$got" == "$want" ]] && return
        sleep 0.05
    done
    fail "makedefault: $what: want '$want', got '$got'"
}

makedefault_wait_handler() {
    local want="$1" got=""
    for _attempt in $(seq 1 100); do
        got=$(makedefault_handler)
        [[ "$got" == "$want" ]] && return
        sleep 0.05
    done
    fail "makedefault: the File manager row never stated $want, it states '$got'"
}

# Stubbed at flea --default, xdg-mime and systemctl, so the claim, the release and the portal restart run and the box
# follows only what the stub's handler file answers: nothing here touches the operator's mimeapps.list, bindings or portal.
case_makedefault() {
    local dir="$fixture_root/makedefault" box="$fixture_root/makedefault-box" real_bin="$flea_bin"
    sandbox_scratch "$dir"
    sandbox_scratch "$box"
    : > "$dir/a.txt"
    mkdir -p "$box/bin" "$box/data/applications"
    local queries="$box/queries.log" handler="$box/handler" mode="$box/mode" restart="$box/restart" runs
    makedefault_log="$box/ran.log"
    makedefault_restart_log="$box/systemctl.log"
    : > "$makedefault_log"
    : > "$queries"
    : > "$makedefault_restart_log"
    printf 'org.gnome.Nautilus.desktop\n' > "$handler"
    printf 'ok\n' > "$mode"
    printf 'ok\n' > "$restart"
    # The row looks for the packaged entry on the XDG data ladder before it offers a claim, as flea --default does.
    printf '[Desktop Entry]\nType=Application\nName=Flea\nExec=flea %%U\n' > "$box/data/applications/com.thisisgm.flea.desktop"

    # Only the two --default shapes are intercepted: the backend and every other mode run the real binary.
    cat > "$box/bin/flea" <<'STUB'
#!/bin/sh
case "$*" in
    --default|"--default off") ;;
    *) exec "$FLEA_MAKEDEFAULT_REAL" "$@" ;;
esac
printf 'RAN %s\n' "$*" >> "$FLEA_MAKEDEFAULT_BOX/ran.log"
# Long enough to read the working state and to press again while it lasts.
sleep 2
mode=$(cat "$FLEA_MAKEDEFAULT_BOX/mode")
case "$mode" in
    refuse) echo "flea: com.thisisgm.flea.desktop is not installed in any applications directory, so there is nothing to make the default; install the package first" >&2; exit 1 ;;
    fail) echo "flea: xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop" >&2; exit 1 ;;
esac
if [ "$*" = --default ]; then
    echo com.thisisgm.flea.desktop > "$FLEA_MAKEDEFAULT_BOX/handler"
    [ "$mode" != partly ] || echo "flea: no portal backend is installed, so the file chooser step was skipped" >&2
    echo "undo both with: flea --default off"
else
    echo org.gnome.Nautilus.desktop > "$FLEA_MAKEDEFAULT_BOX/handler"
fi
STUB
    chmod +x "$box/bin/flea"
    cat > "$box/bin/xdg-mime" <<'STUB'
#!/bin/sh
# Sample input: xdg-mime query default inode/directory
[ "$*" = "query default inode/directory" ] || exec /usr/bin/xdg-mime "$@"
echo query >> "$FLEA_MAKEDEFAULT_BOX/queries.log"
cat "$FLEA_MAKEDEFAULT_BOX/handler"
STUB
    chmod +x "$box/bin/xdg-mime"
    # Every portal call is logged and answered here, so even a wrong argv never reaches the operator's portal.
    cat > "$box/bin/systemctl" <<'STUB'
#!/bin/sh
# Sample input: systemctl --user try-restart xdg-desktop-portal.service
case "$*" in
    *xdg-desktop-portal*) ;;
    *) exec /usr/bin/systemctl "$@" ;;
esac
printf 'SYSTEMCTL %s\n' "$*" >> "$FLEA_MAKEDEFAULT_BOX/systemctl.log"
[ "$*" = "--user try-restart xdg-desktop-portal.service" ] || exit 1
[ "$(cat "$FLEA_MAKEDEFAULT_BOX/restart")" = ok ]
STUB
    chmod +x "$box/bin/systemctl"

    local saved_path="$PATH" saved_dirs="${XDG_DATA_DIRS-}"
    export PATH="$box/bin:$PATH" FLEA_MAKEDEFAULT_BOX="$box" FLEA_MAKEDEFAULT_REAL="$real_bin"
    # Prepended, so the fixture entry is found whether or not a Flea package is installed on this box.
    export XDG_DATA_DIRS="$box/data:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
    flea_bin="$box/bin/flea"
    launch "$dir"
    export PATH="$saved_path"
    if [[ -n "$saved_dirs" ]]; then export XDG_DATA_DIRS="$saved_dirs"; else unset XDG_DATA_DIRS; fi
    unset FLEA_MAKEDEFAULT_BOX FLEA_MAKEDEFAULT_REAL
    flea_bin="$real_bin"
    wait_listing 1

    settings_open_key
    settle
    settings_section about
    makedefault_wait_handler org.gnome.Nautilus.desktop
    # The entry probe runs beside the handler read, and a miss would grey the row after this point.
    settle
    makedefault_wait "false|false||" "About did not open unticked and live over a Nautilus handler"
    ipc settingsModel | jq -e '(map(.id) | index("makeDefault")) as $at | .[$at - 1].label == "File manager" and .[$at].caption == null' >/dev/null \
        || fail "makedefault: the row is not directly under the File manager fact with no caption, got $(ipc settingsModel | jq -c 'map(.label)')"
    shot makedefault-off

    # Keyboard: Space claims, and a second Space while the run lasts runs nothing.
    settings_focus_row makeDefault
    key -k space >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Space did not start the claim"
    key -k space >/dev/null
    makedefault_wait "true|false|Folders, Show in folder and file dialogs open Flea.|foreground" "the claim did not settle ticked"
    [[ "$(makedefault_ran)" == "RAN --default;" ]] \
        || fail "makedefault: the stub ran '$(makedefault_ran)', so a press during the run was not ignored"
    makedefault_wait_handler com.thisisgm.flea.desktop
    [[ "$(makedefault_restarts)" == 1 ]] || fail "makedefault: a claim that went through did not restart the portal once"
    [[ "$(grep -c . "$queries")" == 2 ]] \
        || fail "makedefault: xdg-mime was asked $(grep -c . "$queries") times, so the handler was not read again after the run"
    ipc settingsModel | jq -e '(map(.id) | index("makeDefault")) as $at | .[$at + 1].elide == "right"' >/dev/null \
        || fail "makedefault: the note is not the one eliding line"
    shot makedefault-on

    # Pointer: a click on the row hands folders back, and the box follows the handler read after it.
    settings_click_control makeDefault
    makedefault_wait "true|true|Handing folders back|muted" "a click did not start handing folders back"
    makedefault_wait "false|false||" "handing back did not settle unticked with no note"
    [[ "$(makedefault_ran)" == "RAN --default;RAN --default off;" ]] || fail "makedefault: the stub ran '$(makedefault_ran)'"
    makedefault_wait_handler org.gnome.Nautilus.desktop
    [[ "$(makedefault_restarts)" == 2 ]] || fail "makedefault: a release that went through did not restart the portal"

    # Enter claims too; a build with no portal files says the file dialogs are left out.
    printf 'partly\n' > "$mode"
    key -k Return >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Enter did not start the claim"
    makedefault_wait "true|false|File dialogs need the flea package's portal files.|foreground" "a claim that skipped the chooser step did not say so"
    shot makedefault-partly
    printf 'ok\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|false||" "the partly claim was not handed back"
    [[ "$(makedefault_restarts)" == 4 ]] || fail "makedefault: the partly claim and its release did not each restart the portal"

    # A portal that will not restart keeps the switch as it went and says when file dialogs follow.
    printf 'fail\n' > "$restart"
    key -k space >/dev/null
    makedefault_wait "true|false|File dialogs follow after xdg-desktop-portal restarts.|foreground" "a failed portal restart did not say so"
    shot makedefault-portal
    printf 'ok\n' > "$restart"
    key -k space >/dev/null
    makedefault_wait "false|false||" "the claim with the late portal was not handed back"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: the portal was not restarted after each switch that went through"

    # A failed claim shows flea's own line in the error role, and the box stays what xdg-mime answers.
    printf 'fail\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|false|xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop|error" \
        "a failed claim did not show flea's own line"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: a failed run restarted the portal"
    shot makedefault-failed

    # A refused claim, the build with no desktop entry, leaves the row inert to Space and to a click.
    printf 'refuse\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|true|Install a Flea package to make it the default.|foreground" "a refused claim did not make the row inert"
    runs=$(grep -c . "$makedefault_log")
    key -k space >/dev/null
    settings_click_control makeDefault
    sleep 1
    [[ "$(grep -c . "$makedefault_log")" == "$runs" ]] || fail "makedefault: the inert row still ran '$(makedefault_ran)'"
    makedefault_wait "false|true|Install a Flea package to make it the default.|foreground" "the inert row changed under a press"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: a refused run restarted the portal"
    shot makedefault-unpackaged

    # An action with a live state, never a setting: nothing reached ui.json and no save was refused.
    ! ipc uiSettings | jq -e 'has("makeDefault")' >/dev/null || fail "makedefault: the session state carries makeDefault"
    [[ ! -e "$XDG_STATE_HOME/flea/ui.json" ]] || ! jq -e 'has("makeDefault")' "$XDG_STATE_HOME/flea/ui.json" >/dev/null \
        || fail "makedefault: ui.json carries makeDefault"
    [[ "$(ipc lastMessage)" != "That setting could not be saved." ]] || fail "makedefault: a press was sent to the settings writer"
    key -k Escape >/dev/null
    settle

    printf 'MAKEDEFAULT keyboard=ok pointer=ok enter=ok partly=ok portal=ok failed=ok unpackaged=ok stored=none runs=%s restarts=%s\n' \
        "$runs" "$(makedefault_restarts)"
    kill_flea
}
