import QtQuick
import "." as Flea
import "js/Picker.js" as Picker
import "js/Sort.js" as Sort

// The chooser's column header: the window's own ui/Header.qml over the columns ui/PickerList.qml
// draws. It owns no sort state and asks ui/PickerWindow.qml for an order; whether one may be asked
// for at all is the picker's sortable, which the s and S keys obey too, so a click and a key
// cannot differ.
Flea.Header {
    id: root
    required property var picker
    required property var backend

    hiddenCols: Picker.HIDDEN_COLS
    compactDate: true
    enabled: root.picker.sortable
    // Recent is the desktop's own order and none of the three a mark can describe; see
    // ui/Backend.qml listPaths. While a sort is in flight the mark stays: it moves on the click.
    sortBy: root.picker.recent ? "" : root.backend.sortBy
    sortDesc: root.backend.sortDesc
    onSortRequested: function (key) {
        root.picker.requestSort(Sort.columnOrder(Picker.SORT_ORDERS, root.backend.sortBy, root.backend.sortDesc, key))
    }

    // What tests/picker-native.py clicks, in the shape every other chooser control reports.
    function controls() {
        return [["Sort by Name", "name"], ["Sort by Size", "size"], ["Sort by Modified", "date"]].map(function (entry) {
            return root.picker.control(entry[0], root.cell(entry[1]), root.enabled)
        })
    }
}
