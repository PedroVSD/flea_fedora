.pragma library

// The question a paste or a drop asks before it lands on names that exist, as decisions ui/CollideHost.qml and ui/CollideConfirm.qml wire.

// Left to right, the order h, l and Tab walk.
var BUTTONS = ["cancel", "skip", "keep", "replace"]
var LABELS = { cancel: "Cancel", skip: "Skip", keep: "Keep both", replace: "Replace" }
// Where the card opens, and so what a reflexive Enter takes: the one choice that changes nothing already there.
var START = "keep"
var EXPLAIN = "Replaced items go to Trash, and Undo restores them."
// What a transfer carries when nothing collided: any name that appears after the question is refused.
var NONE = "refuse"
var WAITING = "A transfer is already waiting on its question; try again once it is answered."

// The folder's own name, which is what the title says; the root has no leaf, so it says itself.
function folderName(dest) {
    var path = String(dest || "")
    var cut = path.lastIndexOf("/")
    var leaf = cut >= 0 ? path.substring(cut + 1) : path
    return leaf.length > 0 ? leaf : "/"
}

// "screenshot.png already exists in Pictures" for one, "3 items already exist in Downloads" for several.
function title(names, total, dest) {
    if (total === 1 && names.length > 0)
        return names[0].n + " already exists in " + folderName(dest)
    return total + " items already exist in " + folderName(dest)
}

// The row under the list, empty when every collision is already on it.
function more(total, shown) {
    return total > shown ? "and " + (total - shown) + " more" : ""
}

// h and l stop at the ends like TrashConfirm's pair; Tab and Backtab come round.
function moved(focus, key) {
    var at = BUTTONS.indexOf(focus)
    if (at < 0)
        at = BUTTONS.indexOf(START)
    var last = BUTTONS.length - 1
    if (key === Qt.Key_H || key === Qt.Key_Left)
        return BUTTONS[Math.max(0, at - 1)]
    if (key === Qt.Key_L || key === Qt.Key_Right)
        return BUTTONS[Math.min(last, at + 1)]
    if (key === Qt.Key_Tab)
        return BUTTONS[(at + 1) % BUTTONS.length]
    if (key === Qt.Key_Backtab)
        return BUTTONS[(at + last) % BUTTONS.length]
    return BUTTONS[at]
}

function activates(key) {
    return key === Qt.Key_Return || key === Qt.Key_Enter || key === Qt.Key_Space
}

// One question at a time, open or still in flight: a second is refused out loud rather than replacing the first.
function refusal(opened, pending) {
    return opened || pending !== null ? WAITING : ""
}

// The question names the same sources the transfer will, in the backend's order: a shelf drag's paths, a menu's selection, paths, rows.
function question(request, probe, id) {
    var asked = { c: "collisions", id: id, dest: request.dest }
    if (probe && probe.length > 0)
        asked.paths = probe
    else if (request.menuId)
        asked.menuId = request.menuId
    else if (request.paths && request.paths.length > 0)
        asked.paths = request.paths
    else
        asked.rows = request.rows || []
    return asked
}

// The waiting transfer with the answer on it; the id is what lets the backend cover only the names it listed.
function transfer(request, choice, id) {
    return Object.assign({}, request, { collide: choice, collideId: id })
}
