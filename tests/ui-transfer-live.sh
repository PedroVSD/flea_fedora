#!/usr/bin/env bash
# Sourced by ui.sh after ui-menus.sh, whose menus_expect, menus_shot and menus_same_file this case uses.
# shellcheck disable=SC2034,SC2154 # ui.sh supplies state; sourced helpers consume dynamically scoped locals.

# One MiB, the unit every payload size below is counted in.
transferlive_mib=$((1024 * 1024))
# Microseconds in a second, for the rates the throughput sample divides out.
transferlive_us_per_s=1000000
# Milliseconds in a second, for the expected leg length and the time-left spans.
transferlive_ms_per_s=1000
# The throughput sample: this much fresh random data, then one read/write copy of it.
transferlive_probe_mib=512
# src/backend/copyfile.rs CHUNK, so the sample copies in the same 256 KiB reads and writes Flea's own loop does.
transferlive_chunk=256K
# The payload aims at this long a copy at the sampled rate; the sample runs in page cache, so a real leg only runs longer.
transferlive_target_s=8
# Below this at the sampled rate the run says so, because the cap or the free space bounded the payload.
transferlive_goal_s=5
# The ceiling on one leg's payload, 12 GiB.
transferlive_cap_mib=$((12 * 1024))
# The smallest payload worth running: under 1 GiB a copy can settle before the card has two rate samples.
transferlive_floor_mib=1024
# Kept free on the fixture filesystem beyond this case's own peak, so the rest of the battery still has room.
transferlive_reserve_mib=$((4 * 1024))
# The peak on disk in half payloads: the full file, the half file, and one leg's full-size copy.
transferlive_peak_halves=5
# dd writers per payload file, each at its own offset, because one urandom reader alone is the slow part of the fill.
transferlive_writers=4
# One dd that runs longer than this is wedged rather than slow.
transferlive_dd_timeout_s=600
# src/backend/opsreq.rs PROGRESS_EVERY is 150 ms, so the harness asks at the backend's own beat.
transferlive_poll_s=0.15
# A leg still running after three minutes at any plausible rate is wedged.
transferlive_leg_ms=180000
# The time left has to fall between two readings at least this far apart.
transferlive_fall_span_ms=1000
# The settings every leg starts from: the list view, the stock keys, and no preview reading a payload on its own.
transferlive_ui_state='{"view":"list","keys":"default","display":{"textSize":{"mode":14}},"preview":{"thumbnails":"off","loadOn":"manual"},"menu":{"hidden":[]}}'

# ui/js/Format.js size(): whole bytes below 1000 (a rate keeps its fraction there), else one decimal and an SI unit.
transferlive_size_re='([0-9]+(\.[0-9]+)? B|[0-9]+\.[0-9] (kB|MB|GB|TB))'
# ui/js/Format.js duration(): m:ss below an hour, h:mm:ss from there.
transferlive_duration_re='([1-9][0-9]*:[0-5][0-9]:[0-5][0-9]|[1-5]?[0-9]:[0-5][0-9])'
# ui/js/Transfer.js byteParts() holding a total and a rate: "4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left".
transferlive_full_re="^${transferlive_size_re} of ${transferlive_size_re} · ${transferlive_size_re}/s · ${transferlive_duration_re} left\$"
# Every shape byteParts() can draw: with a total or without one, and a time left only beside both.
transferlive_any_re="^${transferlive_size_re}( of ${transferlive_size_re}| copied| moved) · ${transferlive_size_re}/s( · ${transferlive_duration_re} left)?\$"

# ui/js/Format.js size() for a byte count, 12884901888 becoming "12.9 GB"; whole MiB never sit on the tie where C and toFixed round apart.
transferlive_size() {
    LC_ALL=C awk -v bytes="$1" 'BEGIN {
        split("B kB MB GB TB", unit, " ")
        if (bytes < 1000) { printf "%d B\n", bytes; exit }
        value = bytes
        at = 1
        while (value >= 1000 && at < 5) { value /= 1000; at++ }
        printf "%.1f %s\n", value, unit[at]
    }'
}

# Sample input: "4.2 GB", the moved figure a byte line opens with; prints the bytes it stands for.
transferlive_bytes() {
    LC_ALL=C awk -v figure="$1" 'BEGIN {
        if (split(figure, part, " ") != 2) exit 1
        n = split("B kB MB GB TB", unit, " ")
        scale = 1
        for (at = 1; at <= n; at++) {
            if (part[2] == unit[at]) { printf "%.0f\n", part[1] * scale; exit 0 }
            scale *= 1000
        }
        exit 1
    }'
}

# Sample input: "1:03:05" or "0:04", ui/js/Format.js duration(); prints whole seconds.
transferlive_seconds() {
    local -a field=()
    IFS=: read -r -a field <<< "$1"
    case ${#field[@]} in
        2) printf '%s\n' "$((10#${field[0]} * 60 + 10#${field[1]}))" ;;
        3) printf '%s\n' "$((10#${field[0]} * 3600 + 10#${field[1]} * 60 + 10#${field[2]}))" ;;
        *) return 1 ;;
    esac
}

# Every exit path: stop any payload writer, drain the window, drop the private bus, and delete every payload byte.
transferlive_teardown() {
    local pid drained=1
    for pid in "${transferlive_writer_pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${transferlive_writer_pids[@]}"; do wait "$pid" 2>/dev/null || true; done
    transferlive_writer_pids=()
    ( kill_flea ) || drained=0
    if [[ -n "$transferlive_bus_pid" ]]; then kill "$transferlive_bus_pid" 2>/dev/null || true; fi
    transferlive_bus_pid=""
    if [[ -n "$transferlive_root" ]]; then sandbox_remove "$transferlive_root"; fi
    (( drained )) || fail "transferlive: the window did not drain at teardown; the payload was deleted anyway"
}

# Writes random bytes into each FILE MIB pair, several dd writers per file at their own offsets, and waits for all of them.
transferlive_fill() {
    local file mib part seek count pid status=0
    local -a pids=()
    while (( $# >= 2 )); do
        file=$1 mib=$2
        shift 2
        : > "$file" || fail "transferlive: cannot create $file"
        part=$(( (mib + transferlive_writers - 1) / transferlive_writers ))
        for (( seek = 0; seek < mib; seek += part )); do
            count=$(( mib - seek < part ? mib - seek : part ))
            timeout "$transferlive_dd_timeout_s" dd if=/dev/urandom of="$file" bs=1M seek="$seek" count="$count" \
                conv=notrunc iflag=fullblock status=none &
            pids+=("$!")
            transferlive_writer_pids+=("$!")
        done
    done
    for pid in "${pids[@]}"; do wait "$pid" || status=1; done
    transferlive_writer_pids=()
    (( status == 0 )) || fail "transferlive: a random payload writer failed or ran past $transferlive_dd_timeout_s s"
}

# Sizes the payload from a bounded sample of this filesystem, then writes it: full.bin and half.bin, random throughout.
transferlive_prepare() {
    local probe="$transferlive_root/probe" payload="$transferlive_root/payload" probe_bytes began=0 written=0 copied=0
    local blocks="" block_size="" avail_mib space_mib mib expected_ms filled_ms
    probe_bytes=$((transferlive_probe_mib * transferlive_mib))
    sandbox_scratch "$probe"
    began=$(date +%s%6N)
    timeout "$transferlive_dd_timeout_s" dd if=/dev/urandom of="$probe/random.bin" bs=1M count="$transferlive_probe_mib" \
        iflag=fullblock status=none || fail "transferlive: the random throughput sample could not be written"
    written=$(date +%s%6N)
    timeout "$transferlive_dd_timeout_s" dd if="$probe/random.bin" of="$probe/copy.bin" bs="$transferlive_chunk" status=none \
        || fail "transferlive: the copy throughput sample failed"
    copied=$(date +%s%6N)
    (( written > began && copied > written )) || fail "transferlive: the throughput sample took no measurable time"
    transferlive_random_rate=$(( probe_bytes * transferlive_us_per_s / (written - began) ))
    transferlive_copy_rate=$(( probe_bytes * transferlive_us_per_s / (copied - written) ))
    sandbox_remove "$probe"
    mib=$(( (transferlive_copy_rate * transferlive_target_s + transferlive_mib - 1) / transferlive_mib ))
    (( mib <= transferlive_cap_mib )) || mib=$transferlive_cap_mib
    (( mib >= transferlive_floor_mib )) || mib=$transferlive_floor_mib
    # Sample input: "118234567 4096", stat -f's blocks free to this user and the size they are counted in.
    read -r blocks block_size < <(stat -f -c '%a %S' "$transferlive_root")
    [[ "$blocks" =~ ^[0-9]+$ && "$block_size" =~ ^[0-9]+$ ]] \
        || fail "transferlive: stat -f gave no free-space figure for $transferlive_root: '$blocks' '$block_size'"
    avail_mib=$(( blocks * block_size / transferlive_mib ))
    space_mib=$(( (avail_mib - transferlive_reserve_mib) * 2 / transferlive_peak_halves ))
    (( mib <= space_mib )) || mib=$space_mib
    # Even, so the half payload is a whole number of MiB and the batch total is exactly one full payload.
    mib=$(( mib / 2 * 2 ))
    (( mib >= transferlive_floor_mib )) \
        || fail "transferlive: $avail_mib MiB free on the fixture filesystem holds a $mib MiB payload at most, under the $transferlive_floor_mib MiB floor"
    transferlive_full_bytes=$((mib * transferlive_mib))
    transferlive_half_bytes=$((mib * transferlive_mib / 2))
    expected_ms=$(( transferlive_full_bytes * transferlive_ms_per_s / transferlive_copy_rate ))
    printf 'TRANSFERLIVE_SIZING random_write=%s/s copy=%s/s target=%ss cap=%sMiB free=%sMiB room=%sMiB payload=%sMiB expected_leg=%sms\n' \
        "$(transferlive_size "$transferlive_random_rate")" "$(transferlive_size "$transferlive_copy_rate")" \
        "$transferlive_target_s" "$transferlive_cap_mib" "$avail_mib" "$space_mib" "$mib" "$expected_ms"
    if (( expected_ms < transferlive_goal_s * transferlive_ms_per_s )); then
        printf 'TRANSFERLIVE_NOTE one leg lasts about %s ms at the sampled rate, under the %s s goal, because the cap or the free space bounds the payload\n' \
            "$expected_ms" "$transferlive_goal_s"
    fi
    mkdir "$payload" || fail "transferlive: no payload folder"
    began=$(date +%s%3N)
    transferlive_fill "$payload/full.bin" "$mib" "$payload/half.bin" "$((mib / 2))"
    filled_ms=$(( $(date +%s%3N) - began ))
    [[ "$(stat -c %s "$payload/full.bin")" == "$transferlive_full_bytes" && "$(stat -c %s "$payload/half.bin")" == "$transferlive_half_bytes" ]] \
        || fail "transferlive: the payload files are not the sizes written: $(stat -c '%n %s' "$payload/full.bin" "$payload/half.bin")"
    printf 'TRANSFERLIVE_PAYLOAD full=%s (%s) half=%s (%s) random_fill_ms=%s writers_per_file=%s\n' \
        "$transferlive_full_bytes" "$(transferlive_size "$transferlive_full_bytes")" \
        "$transferlive_half_bytes" "$(transferlive_size "$transferlive_half_bytes")" "$filled_ms" "$transferlive_writers"
}

# A private session bus, the way case_collide runs: gio's trash answers inside this case's XDG_DATA_HOME, never the operator's.
transferlive_start_bus() {
    local -a bus=()
    # Sample output: "unix:path=/tmp/dbus-Ab12Cd34,guid=5f0e9a" then "48213", the address and the daemon's pid, one per line.
    mapfile -t bus < <(dbus-daemon --session --fork --print-address=1 --print-pid=1)
    [[ ${#bus[@]} -eq 2 && "${bus[1]}" =~ ^[0-9]+$ ]] || fail "transferlive: no private session bus, dbus-daemon printed: ${bus[*]}"
    transferlive_bus_pid=${bus[1]}
    export DBUS_SESSION_BUS_ADDRESS="${bus[0]}"
}

# Polls the card until the leg settles, checking each byte line against the leg, its "of" figure, batch size and names.
transferlive_watch() {
    local leg="$1" want_total="$2" state now deadline line text value unit moved total left
    local last_moved=-1 top=-1 top_at=0 top_line="" head_re="^Copying [0-9]+ of $3( · ($4))?\$"
    local -a field=()
    tl_visible=0 tl_polls=0 tl_lines=0 tl_totaled=0 tl_full=0 tl_first="" tl_last="" tl_last_any="" tl_early=""
    tl_fell="" tl_state="" tl_moved_first=-1 tl_moved_last=-1
    deadline=$(( $(date +%s%3N) + transferlive_leg_ms ))
    while :; do
        state=$(ipc statusActivityState) || fail "transferlive: $leg: the activity observer failed"
        now=$(date +%s%3N)
        tl_polls=$((tl_polls + 1))
        # Sample state: {"activities":[{"text":"Copying 1 of 1 · big.bin","running":true}],"errors":0,"notice":"","transferCard":{"visible":true,"byteLine":"4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left"}}
        mapfile -t field < <(jq -r '.transferCard.visible, (.activities | length), .errors, .transferCard.byteLine, .notice, (.activities[0].text // "")' <<< "$state")
        [[ ${#field[@]} -eq 6 ]] || fail "transferlive: $leg: unreadable activity state: $state"
        if [[ "${field[0]}" == true ]]; then tl_visible=1; fi
        text=${field[5]}
        if [[ "$text" == Copying* && ! "$text" =~ $head_re ]]; then
            fail "transferlive: $leg: the headline reads [$text], which names no item this leg copies"
        fi
        line=${field[3]}
        if [[ -n "$line" ]]; then
            tl_lines=$((tl_lines + 1))
            tl_last_any=$line
            [[ "$line" =~ $transferlive_any_re ]] || fail "transferlive: $leg: [$line] is no shape ui/js/Transfer.js byteParts draws"
            # Sample line: "4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left"; its first two words are the moved figure.
            read -r value unit _ <<< "$line"
            moved=$(transferlive_bytes "$value $unit") || fail "transferlive: $leg: unreadable moved figure in [$line]"
            (( moved >= last_moved )) || fail "transferlive: $leg: the moved figure went back, $last_moved bytes then [$line]"
            last_moved=$moved
            if (( tl_moved_first < 0 )); then tl_moved_first=$moved; fi
            tl_moved_last=$moved
            if [[ "$line" == *" of "* ]]; then
                # Sample line: "4.2 GB of 12.9 GB · 2.1 GB/s"; the total sits between " of " and the first " · ".
                total=${line#* of }
                total=${total%% · *}
                [[ "$total" == "$want_total" ]] || fail "transferlive: $leg: the card totals $total, not $want_total: [$line]"
                tl_totaled=$((tl_totaled + 1))
            elif [[ -z "$tl_early" ]]; then
                tl_early=$line
            fi
            if [[ "$line" =~ $transferlive_full_re ]]; then
                tl_full=$((tl_full + 1))
                tl_last=$line
                if [[ -z "$tl_first" ]]; then
                    tl_first=$line
                    menus_shot "transferlive-$leg"
                fi
                # Sample line: "4.2 GB of 12.9 GB · 2.1 GB/s · 0:04 left"; the time left is the word before " left".
                left=${line% left}
                left=$(transferlive_seconds "${left##* }") || fail "transferlive: $leg: unreadable time left in [$line]"
                if [[ -z "$tl_fell" ]] && (( top >= 0 && now - top_at >= transferlive_fall_span_ms && left < top )); then
                    tl_fell="[$top_line] then [$line] $((now - top_at)) ms later"
                fi
                if (( left > top )); then
                    top=$left top_at=$now top_line=$line
                fi
            fi
        fi
        # Settled: nothing running, the card down, and either a completion carrying its undo hint or an error.
        if [[ "${field[1]}" == 0 && "${field[0]}" == false ]] && [[ "${field[4]}" == *" · z undoes" || "${field[2]}" != 0 ]]; then
            tl_state=$state
            return 0
        fi
        (( now < deadline )) || fail "transferlive: $leg: no settled transfer after $((transferlive_leg_ms / transferlive_ms_per_s)) s: $state"
        sleep "$transferlive_poll_s"
    done
}

# A leg's end: the card seen and gone, a line with total, rate and time left drawn, no error, and the leg's own completion.
transferlive_settled() {
    local leg="$1" said="$2"
    (( tl_visible == 1 )) \
        || fail "transferlive: $leg: the copy settled before the card was ever seen; payload $(transferlive_size "$transferlive_full_bytes") at a sampled $(transferlive_size "$transferlive_copy_rate")/s outran the harness: $tl_state"
    (( tl_full > 0 )) \
        || fail "transferlive: $leg: $tl_lines byte lines and none carried a total, a rate and a time left together; the last was [$tl_last_any]"
    (( tl_moved_last > tl_moved_first )) \
        || fail "transferlive: $leg: the moved figure never grew, $tl_moved_first bytes first and $tl_moved_last last"
    jq -e --arg said "$said" '(.activities | length) == 0 and (.transferCard.visible | not) and .errors == 0 and .notice == $said' <<< "$tl_state" >/dev/null \
        || fail "transferlive: $leg: the transfer did not settle as [$said] with no card and no error: $tl_state"
    menus_expect collideState '.opened | not' "$leg: no collision card is left open"
    printf 'TRANSFERLIVE_LEG leg=%s polls=%s lines=%s totaled=%s full=%s first=[%s] last=[%s] fell=%s notice=[%s]\n' \
        "$leg" "$tl_polls" "$tl_lines" "$tl_totaled" "$tl_full" "$tl_first" "$tl_last" "${tl_fell:-none}" "$said"
}

# From the leg's source listing into its to folder, which is where every paste in this case lands.
transferlive_enter_to() {
    local dir="$1" count="$2"
    seek_row_named to
    key -k Return >/dev/null
    wait_path "$dir/to"
    wait_listing "$count"
}

# The destination holds exactly these names, in C order, and nothing a Keep both or a stray partial would add.
transferlive_holds() {
    local dir="$1" want="$2" seen
    seen=$(find "$dir" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ') || fail "transferlive: cannot list $dir"
    [[ "$seen" == "$want " ]] || fail "transferlive: $dir holds [$seen], not [$want]"
}

# Leg one: one big file pasted into an empty folder, the card watched from its first sample to its last.
transferlive_one() {
    local dir="$transferlive_root/one" want
    sandbox_scratch "$dir"
    mkdir "$dir/to" || fail "transferlive: one: no destination folder"
    # A hard link to the full payload: the leg reads the same random bytes without writing them twice.
    ln "$transferlive_root/payload/full.bin" "$dir/big.bin" || fail "transferlive: one: the source could not be linked in"
    want=$(transferlive_size "$transferlive_full_bytes")
    launch "$dir"
    wait_listing 2
    seek_row_named big.bin
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and (.clipboard.paths[0] | endswith("/one/big.bin")) and (.clipboard.cut | not)' \
        "one: big.bin is on the clipboard"
    transferlive_enter_to "$dir" 0
    key p >/dev/null
    transferlive_watch one "$want" 1 'big\.bin'
    transferlive_settled one "Copied 1 item · z undoes"
    [[ -n "$tl_fell" ]] \
        || fail "transferlive: one: the time left never fell across two readings $transferlive_fall_span_ms ms apart; first [$tl_first], last [$tl_last]"
    transferlive_sample_one="[$tl_first] [$tl_last]"
    kill_flea
    transferlive_holds "$dir/to" "big.bin"
    menus_same_file "one: the copy is byte-identical to its source" "$dir/big.bin" "$dir/to/big.bin"
    sandbox_remove "$dir"
}

# Leg two: three big files, one already in the destination, pasted with Skip; the batch total leaves the skipped one out.
transferlive_batch() {
    local dir="$transferlive_root/batch" want every name
    sandbox_scratch "$dir"
    mkdir "$dir/to" || fail "transferlive: batch: no destination folder"
    # a.bin and c.bin are the half payload and b.bin the full one, so the two that copy total exactly one full payload.
    ln "$transferlive_root/payload/half.bin" "$dir/a.bin" || fail "transferlive: batch: a.bin could not be linked in"
    ln "$transferlive_root/payload/full.bin" "$dir/b.bin" || fail "transferlive: batch: b.bin could not be linked in"
    ln "$transferlive_root/payload/half.bin" "$dir/c.bin" || fail "transferlive: batch: c.bin could not be linked in"
    printf 'there\n' > "$dir/to/b.bin" || fail "transferlive: batch: the colliding b.bin could not be written"
    want=$(transferlive_size "$((2 * transferlive_half_bytes))")
    every=$(transferlive_size "$((2 * transferlive_half_bytes + transferlive_full_bytes))")
    [[ "$want" != "$every" ]] || fail "transferlive: batch: the two-file total $want reads the same as all three"
    launch "$dir"
    wait_listing 4
    seek_row_named a.bin
    key v >/dev/null
    key J >/dev/null
    key J >/dev/null
    settle
    [[ "$(ipc selectedIndices)" == "$(row_index_of a.bin),$(row_index_of b.bin),$(row_index_of c.bin)" ]] \
        || fail "transferlive: batch: the three files are not selected, $(ipc selectedIndices) is"
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 3 and (.clipboard.cut | not)' "batch: the three files are on the clipboard"
    transferlive_enter_to "$dir" 1
    key p >/dev/null
    menus_expect collideState '.opened and .title == "b.bin already exists in to" and .names == ["b.bin"] and .more == ""
        and .focus == "keep" and ([.buttons[].visible] | all)' "batch: the paste asks once about b.bin, on Keep both, with every button up"
    key h >/dev/null
    menus_expect collideState '.opened and .focus == "skip"' "batch: h moves the focus to Skip"
    key -k Return >/dev/null
    transferlive_watch batch "$want" 3 'a\.bin|c\.bin'
    transferlive_settled batch "Copied 2 of 3 · 1 skipped · z undoes"
    (( tl_totaled > 0 )) || fail "transferlive: batch: no byte line ever carried the sweep's total; the last was [$tl_last_any]"
    printf 'TRANSFERLIVE_BATCH_TOTAL of=[%s] all_three_would_read=[%s] totaled_lines=%s before_the_sweep=[%s]\n' \
        "$want" "$every" "$tl_totaled" "$tl_early"
    transferlive_sample_batch="[$tl_first] [$tl_last]"
    kill_flea
    transferlive_holds "$dir/to" "a.bin b.bin c.bin"
    [[ "$(cat "$dir/to/b.bin")" == there ]] || fail "transferlive: batch: Skip wrote over the b.bin already there"
    for name in a.bin c.bin; do
        menus_same_file "batch: $name is byte-identical to its source" "$dir/$name" "$dir/to/$name"
    done
    sandbox_remove "$dir"
}

# Leg three: one big file onto a name that exists, with Replace; the old item lands in this case's own Trash.
transferlive_replace() {
    local dir="$transferlive_root/replace" want
    sandbox_scratch "$dir"
    mkdir "$dir/to" || fail "transferlive: replace: no destination folder"
    ln "$transferlive_root/payload/full.bin" "$dir/big.bin" || fail "transferlive: replace: the source could not be linked in"
    printf 'there\n' > "$dir/to/big.bin" || fail "transferlive: replace: the item to replace could not be written"
    want=$(transferlive_size "$transferlive_full_bytes")
    launch "$dir"
    wait_listing 2
    seek_row_named big.bin
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and (.clipboard.paths[0] | endswith("/replace/big.bin")) and (.clipboard.cut | not)' \
        "replace: big.bin is on the clipboard"
    transferlive_enter_to "$dir" 1
    key p >/dev/null
    menus_expect collideState '.opened and .title == "big.bin already exists in to" and .names == ["big.bin"] and .more == ""
        and .focus == "keep" and ([.buttons[].visible] | all)' "replace: the paste asks about big.bin, on Keep both, with every button up"
    key l >/dev/null
    menus_expect collideState '.opened and .focus == "replace"' "replace: l moves the focus to Replace"
    key -k Return >/dev/null
    transferlive_watch replace "$want" 1 'big\.bin'
    transferlive_settled replace "Copied 1 item · z undoes"
    transferlive_sample_replace="[$tl_first] [$tl_last]"
    kill_flea
    [[ "$(cat "$XDG_DATA_HOME/Trash/files/big.bin" 2>/dev/null)" == there ]] \
        || fail "transferlive: replace: the replaced big.bin is not in this case's Trash: $(ls -A "$XDG_DATA_HOME/Trash/files" 2>&1)"
    transferlive_holds "$dir/to" "big.bin"
    menus_same_file "replace: the incoming big.bin is byte-identical to its source" "$dir/big.bin" "$dir/to/big.bin"
    sandbox_remove "$dir"
}

# Issue: GM asked that the transfer card be proven on a real copy, speed and time left included, and operationslive
# misses its window because its 1 GiB sparse file copies before the harness looks. This case sizes a random payload
# from a throughput sample taken on the fixture filesystem itself, then drives three pastes through the real
# clipboard: one big file into an empty folder, a batch with Skip whose total must leave the skipped file out, and a
# Replace whose old item must land in this case's own Trash. Every payload byte is deleted on every exit path.
case_transferlive() {
    local menus_checks=0
    transferlive_root="$fixture_root/transferlive"
    transferlive_bus_pid=""
    transferlive_writer_pids=()
    transferlive_sample_one="" transferlive_sample_batch="" transferlive_sample_replace=""
    trap 'transferlive_teardown' EXIT
    sandbox_make "$transferlive_root"
    seed_ui_state "$transferlive_root/state" "$transferlive_ui_state"
    export XDG_DATA_HOME="$transferlive_root/data"
    mkdir "$XDG_DATA_HOME" || fail "transferlive: no private data home"
    transferlive_prepare
    transferlive_start_bus
    transferlive_one
    transferlive_batch
    transferlive_replace
    printf 'TRANSFERLIVE copy=%s/s random_write=%s/s payload=%s half=%s one=%s batch=%s replace=%s\n' \
        "$(transferlive_size "$transferlive_copy_rate")" "$(transferlive_size "$transferlive_random_rate")" \
        "$(transferlive_size "$transferlive_full_bytes")" "$(transferlive_size "$transferlive_half_bytes")" \
        "$transferlive_sample_one" "$transferlive_sample_batch" "$transferlive_sample_replace"
    transferlive_teardown
    trap - EXIT
    [[ ! -e "$transferlive_root" ]] || fail "transferlive: $transferlive_root survived the teardown"
}
