import QtQuick

// One edge of a menu that scrolls, fading the surface out where more rows wait: the Menus board's "Edges fade to say so".
Rectangle {
    width: parent.width
    height: Theme.spacing.gap
    gradient: Gradient {
        GradientStop { position: 0; color: Theme.color.surface }
        GradientStop { position: 1; color: Qt.rgba(Theme.color.surface.r, Theme.color.surface.g, Theme.color.surface.b, 0) }
    }
}
