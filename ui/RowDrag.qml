import QtQuick
import "js/Drag.js" as DragOps

// Delegate input and drop eligibility share a view-owned FileDrag; no platform object lives in this row.
Item {
    id: root

    required property var session
    required property int listingIndex
    required property var row
    readonly property var pane: root.session.pane

    anchors.fill: parent
    enabled: root.pane !== null && !root.pane.listInFlight && root.listingIndex >= 0 && !!root.row

    DragHandler {
        id: lift
        enabled: root.pane !== null && !root.pane.renamePending && root.pane.renamingIndex !== root.listingIndex
        target: null
        // Preserve List's grab contract: the Flickable must not take the row drag after its threshold.
        grabPermissions: PointerHandler.CanTakeOverFromItems | PointerHandler.CanTakeOverFromHandlersOfDifferentType | PointerHandler.ApprovesTakeOverByHandlersOfSameType
        onActiveChanged: if (active) root.session.liftBegan(root.listingIndex, lift.centroid)
        onCentroidChanged: if (active) root.session.liftMoved(lift.centroid)
    }

    DropArea {
        anchors.fill: parent
        keys: [DragOps.ROWS_MIME, "text/uri-list"]
        onEntered: function (drag) {
            var marker = drag.getDataAsString(DragOps.ROWS_MIME)
            var ok = root.row && root.row.d === true
            if (ok)
                ok = DragOps.hasPaths(drag.urls)
                   ? DragOps.canDropInto(marker, drag.urls, root.pane.join(root.pane.path, root.row.n))
                   : DragOps.canDropByIndex(marker, root.pane.path, root.session.dragRows, root.listingIndex)
            if (!ok) {
                drag.accepted = false
                return
            }
            root.session.dropIndex = root.listingIndex
            root.session.enterTarget(marker, drag.urls, root.row.n, root.row.v,
                                     drag.getDataAsString(DragOps.SHELF_MIME))
        }
        onPositionChanged: function (drag) {
            if (root.session.dropIndex === root.listingIndex)
                root.session.showTarget(root.row.n, root.row.v)
        }
        onExited: {
            if (root.session.dropIndex === root.listingIndex) {
                root.session.dropIndex = -1
                root.session.leaveTarget()
            }
            if (root.session.dragRows.length === 0) root.session.dragCopy = false
        }
        onDropped: function (drop) {
            if (root.dropped(drop.getDataAsString(DragOps.ROWS_MIME), drop.urls, drop.getDataAsString(DragOps.SHELF_MIME)))
                drop.accept(Qt.CopyAction)
        }
    }

    // The drop apart from its platform event, which tests/js/collide.js cannot build: answers whether it was taken.
    function dropped(marker, urls, shelf) {
        var accepted = false
        if (root.row && root.row.d === true) {
            if (DragOps.hasPaths(urls))
                accepted = DragOps.dropInto(root.pane, marker, urls, root.pane.join(root.pane.path, root.row.n), root.row.v, shelf)
            else if (DragOps.canDropByIndex(marker, root.pane.path, root.session.dragRows, root.listingIndex))
                accepted = DragOps.drop(root.pane, root.session.dragRows, root.listingIndex,
                    root.session.verbAt(marker, root.row) === "copy", root.session.dragListing)
        }
        root.session.dropIndex = -1
        root.session.dragCopy = false
        root.session.leaveTarget()
        return accepted
    }
}
