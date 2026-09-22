pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "js/MakeDefault.js" as MakeDefault

// Settings > About's Make Flea the default, shared by every window: the handler xdg-mime answers, whether this build has a desktop entry, `flea --default [off]` and the portal restart after it; ui/js/MakeDefault.js holds the states and the words.
QtObject {
    id: root

    readonly property string binary: Quickshell.env("FLEA_BIN") || "flea"
    // The one answer the File manager row states and the box is ticked by.
    property string handler: ""
    property var claim: MakeDefault.idle()

    // About opening: the handler, and whether there is an entry for flea --default to point at.
    function read() {
        reader.running = true
        prober.command = MakeDefault.probeCommand(MakeDefault.entryPaths({
            HOME: Quickshell.env("HOME") || "",
            XDG_DATA_HOME: Quickshell.env("XDG_DATA_HOME") || "",
            XDG_DATA_DIRS: Quickshell.env("XDG_DATA_DIRS") || ""
        }))
        prober.running = true
    }

    // Enter, Space or a click on the row; a press during a run, or on a build with no entry, does nothing.
    function toggle() {
        var args = MakeDefault.press(root.handler, root.claim)
        if (args === null || runner.running)
            return
        root.claim = MakeDefault.started(root.claim, args)
        runner.command = [root.binary].concat(args)
        runner.running = true
    }

    property var readQuery: Process {
        id: reader
        command: ["xdg-mime", "query", "default", "inode/directory"]
        stdout: StdioCollector { waitForEnd: true }
        // Sample output: com.thisisgm.flea.desktop
        onExited: function (code) {
            root.handler = code === 0 ? reader.stdout.text.trim() : ""
            // Another window's About can read while a run is still going, and that answer ends nothing.
            if (root.claim.reading)
                root.claim = MakeDefault.settled(root.claim)
        }
    }

    property var probeQuery: Process {
        id: prober
        onExited: function (code) { root.claim = MakeDefault.probed(root.claim, code === 0) }
    }

    // Asynchronous like every Process, so the window keeps drawing while flea rewrites the four files.
    property var runQuery: Process {
        id: runner
        stderr: StdioCollector { waitForEnd: true }
        onExited: function (code) {
            root.claim = MakeDefault.finished(root.claim, code, runner.stderr.text)
            reader.running = true
            if (root.claim.restarting)
                restarter.running = true
        }
    }

    // Only this switch restarts the portal; flea --default on a command line prints the hint instead.
    property var restartQuery: Process {
        id: restarter
        command: MakeDefault.RESTART
        onExited: function (code) { root.claim = MakeDefault.restarted(root.claim, code === 0) }
    }
}
