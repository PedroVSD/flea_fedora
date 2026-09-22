.pragma library

// Settings > About's Make Flea the default: which state the row is in, the note under it and what a press runs; ui/DefaultClaim.qml runs the processes.

// The row's id, which ui/SettingsPanel.qml routes to the run instead of writing it into ui.json.
var ROW_ID = "makeDefault"
// The entry flea --default points the desktop at; the box is ticked exactly when xdg-mime answers it.
var DESKTOP_ID = "com.thisisgm.flea.desktop"
// src/main.rs claim_both() says this and still exits 0 when a build has no flea.portal to route file dialogs to.
var SKIPPED = "no portal backend is installed, so the file chooser step was skipped"
// src/defaults.rs claim() refuses with this when the running binary has no desktop entry to point at.
var UNINSTALLED = "is not installed in any applications directory"
// The portal reads its routing once, at startup; try-restart leaves a portal that is not running alone.
var RESTART = ["systemctl", "--user", "try-restart", "xdg-desktop-portal.service"]

// Nothing run in this process yet; an entry is assumed until the probe or a refusal says otherwise.
function idle() {
    return { running: false, claiming: false, reading: false, restarting: false, outcome: "", error: "", packaged: true }
}

// A run in flight, or a build with no entry, takes no press.
function inert(claim) {
    return claim.running || claim.packaged === false
}

// flea's arguments for a press, or null while the row is inert.
function press(handler, claim) {
    if (inert(claim))
        return null
    return handler === DESKTOP_ID ? ["--default", "off"] : ["--default"]
}

// A new run drops the last one's outcome; which way it goes picks the working note.
function started(claim, args) {
    return { running: true, claiming: args.length === 1, reading: false, restarting: false, outcome: "", error: "",
             packaged: claim.packaged }
}

// Still running until the handler is read again and a switch that went through has restarted the portal.
// Sample stderr: "flea: no portal backend is installed, so the file chooser step was skipped\n"
function finished(claim, code, stderr) {
    var text = stderr || ""
    var next = Object.assign({}, claim, { reading: true, restarting: code === 0 })
    if (code === 0) {
        next.outcome = text.indexOf(SKIPPED) >= 0 ? "partly" : ""
        return next
    }
    if (text.indexOf(UNINSTALLED) >= 0) {
        next.packaged = false
        return next
    }
    next.outcome = "failed"
    next.error = errorLine(text, code, claim.claiming)
    return next
}

// xdg-mime's stderr reaches flea's own, so flea's line is the one that starts with its name.
// Sample stderr: "xdg-mime: no method available\nflea: xdg-mime default exited 3\n"
function errorLine(stderr, code, claiming) {
    var lines = stderr.split("\n").map(function (line) { return line.trim() })
    var own = lines.filter(function (line) { return line.indexOf("flea: ") === 0 })[0]
    if (own !== undefined)
        return own.substring(6)
    var first = lines.filter(function (line) { return line.length > 0 })[0]
    return first !== undefined ? first : (claiming ? "flea --default" : "flea --default off") + " exited " + code
}

// The handler has been read again; the run is over unless the portal restart still is.
function settled(claim) {
    return Object.assign({}, claim, { reading: false, running: claim.restarting })
}

// A portal that would not restart keeps the switch as it went; partly keeps its own note, whose dialogs never follow.
function restarted(claim, ok) {
    var outcome = !ok && claim.outcome === "" ? "portal" : claim.outcome
    return Object.assign({}, claim, { restarting: false, running: claim.reading, outcome: outcome })
}

function probed(claim, found) {
    return Object.assign({}, claim, { packaged: found })
}

// One of off, on, working, partly, portal, failed, unpackaged.
function state(handler, claim) {
    if (claim.packaged === false)
        return "unpackaged"
    if (claim.running)
        return "working"
    if (claim.outcome === "failed")
        return "failed"
    if (handler === DESKTOP_ID && claim.outcome === "partly")
        return "partly"
    if (claim.outcome === "portal")
        return "portal"
    return handler === DESKTOP_ID ? "on" : "off"
}

// [words, role] for the one line under the row, or null when off says nothing.
function note(handler, claim) {
    var now = state(handler, claim)
    if (now === "on")
        return ["Folders, Show in folder and file dialogs open Flea.", "foreground"]
    if (now === "working")
        return [claim.claiming ? "Making Flea the default" : "Handing folders back", "muted"]
    if (now === "partly")
        return ["File dialogs need the flea package's portal files.", "foreground"]
    if (now === "portal")
        return ["File dialogs follow after xdg-desktop-portal restarts.", "foreground"]
    if (now === "failed")
        return [claim.error, "error"]
    if (now === "unpackaged")
        return ["Install a Flea package to make it the default.", "foreground"]
    return null
}

// The check row and its note for About's This box group; the note elides on one line rather than wrapping.
function rows(handler, claim) {
    var live = claim || idle()
    var out = [{ kind: "check", id: ROW_ID, label: "Make Flea the default", on: handler === DESKTOP_ID, inert: inert(live) }]
    var line = note(handler, live)
    if (line !== null)
        out.push({ kind: "hint", label: line[0], role: line[1], elide: "right" })
    return out
}

// src/userfile.rs data_file()'s ladder: the data home, then every data dir, an empty variable read as unset.
// Sample env: { HOME: "/home/gm", XDG_DATA_HOME: "", XDG_DATA_DIRS: "/usr/local/share:/usr/share" }
function entryPaths(env) {
    var home = env.XDG_DATA_HOME || (env.HOME ? env.HOME + "/.local/share" : "")
    var dirs = (env.XDG_DATA_DIRS || "/usr/local/share:/usr/share").split(":")
    return [home].concat(dirs)
        .filter(function (dir) { return dir.length > 0 })
        .map(function (dir) { return dir + "/applications/" + DESKTOP_ID })
}

// data_file()'s regular-file test; each path is an argument, so no path reaches the shell as code.
function probeCommand(paths) {
    return ["sh", "-c", "for f; do [ -f \"$f\" ] && exit 0; done; exit 1", "sh"].concat(paths)
}
