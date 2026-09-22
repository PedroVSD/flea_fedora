.pragma library

.import "Eject.js" as Eject
.import "Mounts.js" as Mounts

// What the rail does with a key, split out of Focus.js at its 300-line hard cap the same way
// ui/js/PreviewKeys.js was: Focus.js decides which surface owns a key, and this is the surface.

// The rail answers ten of the key table's action names and ignores the rest while it has focus.
function act(action, root, sidebar) {
    // Any other rail key ends a mount-first open's claim on the focus.
    sidebar.focusOnOpen = false
    switch (action) {
    case "cursorDown": sidebar.cursorIndex = Math.min(sidebar.entries.length - 1, sidebar.cursorIndex + 1); return
    case "cursorUp": sidebar.cursorIndex = Math.max(0, sidebar.cursorIndex - 1); return
    // The sheet advertises g and G as first and last row, and the rail is a cursored list too, so
    // they answered nothing here while every other cursor key worked.
    case "cursorFirst": sidebar.cursorIndex = 0; return
    case "cursorLast": sidebar.cursorIndex = Math.max(0, sidebar.entries.length - 1); return
    // activate(), not opened(path), so a Network entry mounts first and focus follows the open into the folder.
    case "open":
        if (sidebar.entries.length === 0) return
        var entry = sidebar.entries[sidebar.cursorIndex]
        sidebar.activate(sidebar.cursorIndex)
        // An unmounted row only starts its mount, so focus waits for its open to land (ui/PaneRail.qml).
        if (entry && entry.mounted === false) sidebar.focusOnOpen = true
        else root.focusView = "list"
        return
    // Focus.LIST's own value, written out because importing Focus.js back would be a cycle.
    case "escape":
        if (root.statusBar && root.statusBar.escapePressed()) return
        root.focusView = "list"; return
    case "addNetwork": sidebar.addRequested(); return
    // Favorites are not offered: Sidebar.startRename ignores an index outside the Network group.
    case "rename": sidebar.startRename(sidebar.cursorIndex); return
    // Eject and Unmount are menu rows, so this opens the menu rather than inventing a second route.
    case "menu":
        if (sidebar.entries[sidebar.cursorIndex] && sidebar.entries[sidebar.cursorIndex].kind === "trash") sidebar.openCursorMenu()
        else Mounts.raiseMenu(root, sidebar)
        return
    case "eject": Eject.release(root, sidebar, true); return
    }
}
