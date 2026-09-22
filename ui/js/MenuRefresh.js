.pragma library

// Where the keyboard cursor lands when a provider refresh rebuilds an open menu under it; it came out of ui/js/Menu.js whole, and ui/ContextMenu.qml is its caller.

// Preserve action and peer identity when refreshed capabilities change the inventory beneath the keyboard cursor.
function refreshedCursor(previous, next, cursor, submenuRow, submenuCursor) {
    var action = previous[cursor] ? previous[cursor].action : ""
    var selected = action ? next.findIndex(function(entry) { return entry.action === action }) : -1
    if (selected < 0) {
        selected = Math.min(Math.max(0, cursor), next.length - 1)
        while (selected < next.length && selected >= 0 && (next[selected].separator || next[selected].disabled)) selected++
        if (selected === next.length) {
            selected--
            while (selected >= 0 && (next[selected].separator || next[selected].disabled)) selected--
        }
    }
    var oldSubmenu = previous[submenuRow]
    var subRow = oldSubmenu ? next.findIndex(function(entry) { return entry.action === oldSubmenu.action }) : -1
    var sub = subRow >= 0 ? next[subRow] : null
    var oldTarget = oldSubmenu && oldSubmenu.submenu ? oldSubmenu.submenu[submenuCursor] : null
    var target = sub && !sub.disabled && oldTarget ? (sub.submenu || []).findIndex(function(entry) { return entry.id === oldTarget.id && !entry.disabled }) : -1
    return { cursor: selected, submenuRow: target >= 0 ? subRow : -1, submenuCursor: Math.max(0, target) }
}
