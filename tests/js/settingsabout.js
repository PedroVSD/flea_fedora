.import "../../ui/js/Settings.js" as Settings
.import "../../ui/js/Update.js" as Update

// The About section's Updates group: the Update Flea row, the switch that governs the automatic checks, and the note line.

function find(rows, id) {
    return rows.filter(function (row) { return row.id === id })[0] || {}
}

function about(update, data) {
    return Settings.rows("about", { about: { update: update }, data: data || {} })
}

function answered(line) {
    return Update.answered(Update.checking(Update.idle()), line, 1000)
}

// The rows between the Updates heading and the next one, as kind:label, for a status.
function group(update) {
    var rows = about(update)
    var labels = rows.map(function (row) { return row.kind + ":" + row.label })
    var from = labels.indexOf("group:Updates") + 1
    return labels.slice(from, labels.indexOf("group:This box")).join(",")
}

function run(check) {
    var rows = about(undefined)
    check("Updates holds Update Flea then Check automatically, and nothing else while idle",
          group(undefined), "action:Update Flea,check:Check automatically")
    check("the static Update owner fact is gone", rows.filter(function (row) { return row.label === "Update owner" }).length, 0)
    var row = find(rows, "updateFlea")
    check("the row wears the download mark, the one the old fact wore", row.glyph, "download")
    check("idle offers the check in the foreground, with a chevron", row.value + "|" + row.role + "|" + row.inert, "Check|live|false")
    check("and is a stop the cursor rests on", Settings.focusable(row), true)
    var available = find(about(answered("available opr 0.3.3-1 0.3.4-1")), "updateFlea")
    check("an update reads in the accent", available.value + "|" + available.role, "0.3.4 available|accent")
    var checking = find(about(Update.checking(Update.idle())), "updateFlea")
    check("a running check is muted, inert, and still a stop so the cursor does not jump",
          checking.value + "|" + checking.role + "|" + checking.inert + "|" + Settings.focusable(checking), "Checking||true|true")
    var rolling = find(about(answered("unchecked git - -")), "updateFlea")
    check("a rolling build is a fact the cursor steps over", rolling.kind + "|" + rolling.value + "|" + Settings.focusable(rolling),
          "fact|flea-git · rolling|false")

    var launched = Update.launchedFrom(answered("available opr 0.3.3-1 0.3.4-1"), true)
    check("after a launch the group ends on the restart note",
          group(launched), "action:Update Flea,check:Check automatically,hint:Restart Flea when the update finishes")
    var note = about(launched).filter(function (row) { return row.kind === "hint" && row.label.indexOf("Restart") === 0 })[0] || {}
    check("which is in the accent and takes no cursor", note.role + "|" + Settings.focusable(note), "accent|false")
    check("offline explains itself in the foreground",
          about(answered("failed aur 0.3.3-1 -")).filter(function (row) { return row.kind === "hint" })
              .map(function (row) { return row.label + "|" + row.role }).join(""), "Offline: Omarchy update still opens|foreground")

    var auto = find(rows, "updates.autoCheck")
    check("the switch carries no mark and names both triggers in its caption",
          String(auto.glyph) + "|" + auto.caption, "undefined|when About opens, every 6 h")
    check("it ships on", auto.on, true)
    check("and reads a stored false as off", find(about(undefined, { updates: { autoCheck: false } }), "updates.autoCheck").on, false)
    check("it is a check the cursor stops on, which ui/SettingsPanel.qml writes by its own id",
          Settings.focusable(auto) + "|" + auto.kind, "true|check")

    check("the two web rows carry their own destinations",
          find(rows, "reportIssue").url + "|" + find(rows, "support").url,
          "https://github.com/thisisgm/flea/issues|https://github.com/sponsors/thisisgm")
    check("and no other row does", rows.filter(function (r) { return r.url !== undefined }).length, 2)
}
