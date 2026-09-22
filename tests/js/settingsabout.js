.import "../../ui/js/Settings.js" as Settings
.import "../../ui/js/Update.js" as Update
.import "../../ui/js/MakeDefault.js" as MakeDefault

// The About section's Updates group: the Update Flea row, the switch that governs the automatic checks, and the note line; then This box's Make Flea the default row.

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
    runDefault(check)
}

var FLEA = "com.thisisgm.flea.desktop"
var NAUTILUS = "org.gnome.Nautilus.desktop"
var CLAIM = ["--default"]
var RELEASE = ["--default", "off"]
var REFUSAL = "flea: com.thisisgm.flea.desktop is not installed in any applications directory, so there is nothing to make the default; install the package first\n"

function box(handler, claim) {
    return Settings.rows("about", { about: { handler: handler, claim: claim } })
}

// The rows from the File manager fact to Keyboard sheet, as kind:label, the note included when there is one.
function boxGroup(handler, claim) {
    var labels = box(handler, claim).map(function (row) { return row.kind + ":" + row.label })
    var from = labels.indexOf("fact:File manager")
    return labels.slice(from, labels.indexOf("action:Keyboard sheet")).join(",")
}

// The line under the row, or {} when the next row is not a note.
function noteOf(handler, claim) {
    var rows = box(handler, claim)
    var next = rows[rows.map(function (row) { return row.id }).indexOf("makeDefault") + 1]
    return next.kind === "hint" ? next : {}
}

// The row and its note as on|inert|note|role|elide, the three note fields empty when there is none.
function look(handler, claim) {
    var row = find(box(handler, claim), "makeDefault"), note = noteOf(handler, claim)
    return [row.on, row.inert, note.label || "", note.role || "", note.elide || ""].join("|")
}

// A run that has exited, whose handler has been read again, and whose portal restart, if it asked for one, answered.
function after(args, code, stderr, restartOk) {
    var read = MakeDefault.settled(MakeDefault.finished(MakeDefault.started(MakeDefault.idle(), args), code, stderr))
    return read.restarting ? MakeDefault.restarted(read, restartOk !== false) : read
}

function runDefault(check) {
    check("the row sits directly under the File manager fact, which stays a fact",
          boxGroup(NAUTILUS, undefined), "fact:File manager,check:Make Flea the default")
    var row = find(box(NAUTILUS, undefined), "makeDefault")
    check("it is the label and the box alone, with no caption and no mark", String(row.caption) + "|" + String(row.glyph), "undefined|undefined")
    check("a check the cursor stops on", Settings.focusable(row) + "|" + row.kind, "true|check")
    check("off: unticked, live, and no note", look(NAUTILUS, undefined), "false|false|||")
    check("off: a press claims", JSON.stringify(MakeDefault.press(NAUTILUS, MakeDefault.idle())), JSON.stringify(CLAIM))
    check("nothing answered reads as off, not as Flea", look("", undefined), "false|false|||")

    check("on: ticked by xdg-mime's answer, with the note in the foreground on one eliding line", look(FLEA, MakeDefault.idle()),
          "true|false|Folders, Show in folder and file dialogs open Flea.|foreground|right")
    check("on: a press hands folders back", JSON.stringify(MakeDefault.press(FLEA, MakeDefault.idle())), JSON.stringify(RELEASE))
    check("the note is a line the cursor steps over", Settings.focusable(noteOf(FLEA, undefined)), false)

    var claiming = MakeDefault.started(MakeDefault.idle(), CLAIM)
    check("working on a claim: inert, the box still the truth, the note muted",
          look(NAUTILUS, claiming), "false|true|Making Flea the default|muted|right")
    check("working on a release says so", look(FLEA, MakeDefault.started(MakeDefault.idle(), RELEASE)), "true|true|Handing folders back|muted|right")
    check("a press while a run is in flight runs nothing", MakeDefault.press(NAUTILUS, claiming), null)
    check("and the row stays a stop so the cursor does not jump", Settings.focusable(find(box(NAUTILUS, claiming), "makeDefault")), true)
    check("an exited run is still working until the handler is read again",
          MakeDefault.state(NAUTILUS, MakeDefault.finished(claiming, 0, "")), "working")
    check("the re-read answer decides the box, not the press", look(NAUTILUS, after(CLAIM, 0, "")), "false|false|||")

    check("the portal restart is exactly systemctl's try-restart of the user unit", MakeDefault.RESTART.join(" "),
          "systemctl --user try-restart xdg-desktop-portal.service")
    check("a claim, a partly claim and a release that went through each ask for it",
          [MakeDefault.finished(claiming, 0, "").restarting,
           MakeDefault.finished(claiming, 0, "flea: no portal backend is installed, so the file chooser step was skipped\n").restarting,
           MakeDefault.finished(MakeDefault.started(MakeDefault.idle(), RELEASE), 0, "").restarting].join("|"), "true|true|true")
    check("a failed run and a refused one do not",
          MakeDefault.finished(claiming, 1, "flea: no\n").restarting + "|" + MakeDefault.finished(claiming, 1, REFUSAL).restarting, "false|false")
    var exited = MakeDefault.finished(claiming, 0, "")
    check("still working after the re-read while the restart runs", MakeDefault.state(FLEA, MakeDefault.settled(exited)), "working")
    check("and after the restart while the re-read runs", MakeDefault.state(FLEA, MakeDefault.restarted(exited, true)), "working")
    check("either order ends the run",
          MakeDefault.settled(MakeDefault.restarted(exited, true)).running + "|" + MakeDefault.restarted(MakeDefault.settled(exited), true).running,
          "false|false")
    var late = after(CLAIM, 0, "", false)
    check("a restart that failed keeps the claim and says when file dialogs follow", look(FLEA, late),
          "true|false|File dialogs follow after xdg-desktop-portal restarts.|foreground|right")
    check("a release keeps its unticked box with the same line", look(NAUTILUS, after(RELEASE, 0, "", false)),
          "false|false|File dialogs follow after xdg-desktop-portal restarts.|foreground|right")
    check("the next run drops it", MakeDefault.started(late, RELEASE).outcome, "")

    var skipped = "flea: no portal backend is installed, so the file chooser step was skipped\n"
    check("partly: folders claimed, the chooser step skipped", look(FLEA, after(CLAIM, 0, skipped)),
          "true|false|File dialogs need the flea package's portal files.|foreground|right")
    check("partly only while Flea is still the answer", look(NAUTILUS, after(CLAIM, 0, skipped)), "false|false|||")
    check("partly keeps its own note when the restart fails, since its file dialogs never follow",
          look(FLEA, after(CLAIM, 0, skipped, false)), "true|false|File dialogs need the flea package's portal files.|foreground|right")

    var failed = after(CLAIM, 1, "xdg-mime: warning from xdg-mime itself\nflea: xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop\n")
    check("failed: flea's own first line without its prefix, in the error role, the box the re-read truth",
          look(NAUTILUS, failed), "false|false|xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop|error|right")
    check("failed: the next press tries again", JSON.stringify(MakeDefault.press(NAUTILUS, failed)), JSON.stringify(CLAIM))
    check("a failed release keeps the box ticked when Flea is still the answer", look(FLEA, after(RELEASE, 1, "flea: a half failed\n")),
          "true|false|a half failed|error|right")
    check("a line flea did not prefix is still better than nothing", MakeDefault.errorLine("something broke\n", 1, true), "something broke")
    check("silence names the command and its status", MakeDefault.errorLine("", 2, false), "flea --default off exited 2")
    check("a new run drops the last one's error", MakeDefault.started(failed, CLAIM).error + "|" + MakeDefault.started(failed, CLAIM).outcome, "|")

    var refused = MakeDefault.finished(claiming, 1, REFUSAL)
    var unpackaged = "false|true|Install a Flea package to make it the default.|foreground|right"
    check("unpackaged from the refusal, before the re-read lands", look(NAUTILUS, refused), unpackaged)
    check("and after it", look(NAUTILUS, MakeDefault.settled(refused)), unpackaged)
    check("an unpackaged press runs nothing", MakeDefault.press(NAUTILUS, MakeDefault.settled(refused)), null)
    var probedOut = MakeDefault.probed(MakeDefault.idle(), false)
    check("unpackaged from the probe, before any press", look(NAUTILUS, probedOut), unpackaged)
    check("and an entry found later makes the row live again", look(NAUTILUS, MakeDefault.probed(probedOut, true)), "false|false|||")

    check("the probe walks src/userfile.rs data_file()'s ladder",
          MakeDefault.entryPaths({ HOME: "/home/gm", XDG_DATA_HOME: "", XDG_DATA_DIRS: "" }).join(","),
          "/home/gm/.local/share/applications/" + FLEA + ",/usr/local/share/applications/" + FLEA + ",/usr/share/applications/" + FLEA)
    check("a set data home and data dirs replace the defaults, and empty segments are skipped",
          MakeDefault.entryPaths({ HOME: "/home/gm", XDG_DATA_HOME: "/d/home", XDG_DATA_DIRS: "/d/a::/d/b" }).join(","),
          "/d/home/applications/" + FLEA + ",/d/a/applications/" + FLEA + ",/d/b/applications/" + FLEA)
    check("with no HOME there is no data home to look in", MakeDefault.entryPaths({ XDG_DATA_DIRS: "/d/a" }).join(","), "/d/a/applications/" + FLEA)
    var probe = MakeDefault.probeCommand(["/a b/x.desktop", "$(y)"])
    check("each path is its own argument and never part of the script",
          probe.slice(0, 2).concat(probe.slice(3)).join("|") + "|" + (probe[2].indexOf("a b") < 0), "sh|-c|sh|/a b/x.desktop|$(y)|true")
}
