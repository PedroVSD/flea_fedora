.import "../../ui/js/Scroll.js" as Scroll

// The wheel arithmetic behind ui/FastScrollHandler.qml: the distance a notch or a touchpad delta
// moves, the bounds a write is kept inside, and when an event counts as consumed.
function run(check) {
    // A notch: 120 units, the platform's lines, the pixels a line is worth, and the multiplier.
    check("one notch down moves lines times notch pixels times the multiplier",
          Scroll.distance(0, -120, 3, 24, 4), -288)
    check("one notch up moves the same distance the other way", Scroll.distance(0, 120, 3, 24, 4), 288)
    check("two notches move twice", Scroll.distance(0, -240, 3, 24, 4), -576)
    // The one fallback: the handler passes the platform's hint raw, and no lines means Qt's own three.
    check("a platform that reports no lines moves Qt's three", Scroll.distance(0, -120, 0, 24, 4), -288)
    check("and so does one that reports a negative count", Scroll.distance(0, -120, -1, 24, 4), -288)
    // A touchpad hands pixels, which win over any angle that rides along and move one to one.
    check("a pixel delta moves one to one, no multiplier", Scroll.distance(-10, -120, 3, 24, 4), -10)
    check("a fractional pixel delta keeps its fraction", Scroll.distance(-2.5, 0, 3, 24, 4), -2.5)
    check("no delta at all moves nothing", Scroll.distance(0, 0, 3, 24, 4), 0)
    check("garbage reads as no movement", Scroll.distance("x", undefined, 3, 24, 4), 0)

    // Bounds: the origin and the last page, and a short content pinned to the origin.
    check("a write above the origin lands on it", Scroll.bounded(-50, 0, 1000, 400), 0)
    check("a write past the end lands on the last page", Scroll.bounded(5000, 0, 1000, 400), 600)
    check("a write inside stays where it was asked", Scroll.bounded(250, 0, 1000, 400), 250)
    check("a content shorter than the view pins to the origin", Scroll.bounded(100, 0, 300, 400), 0)
    check("an origin below zero is honoured", Scroll.bounded(-100, -20, 1000, 400), -20)
    check("an unknown content height reads as empty", Scroll.bounded(100, 0, undefined, 400), 0)

    // Consumed only when the content moved, so an event at an edge keeps propagating.
    check("a moved content consumes the event", Scroll.moved(100, 388), true)
    check("a content that did not move does not", Scroll.moved(600, 600), false)
    check("a sub-pixel jitter does not count as movement", Scroll.moved(600, 600.005), false)

    // A scrollbar describes the viewport, so a scale listing keeps the 24 px minimum handle everywhere travel is measured.
    check("fitting content has no scroll range", Scroll.range(400, 400), 0)
    check("a listing shorter than the viewport has no range either", Scroll.range(1, 400), 0)
    check("overflow is the content left below one viewport", Scroll.range(1000, 400), 600)
    check("a proportional handle names the visible fraction", Scroll.handleLength(500, 1000, 400, 24), 200)
    check("a scale listing keeps a usable minimum handle", Scroll.handleLength(500, 3700000, 500, 24), 24)
    check("the top maps to the top of the track", Scroll.handleOffset(0, 0, 1000, 400, 500, 24), 0)
    check("the last page maps to the end of the track", Scroll.handleOffset(600, 0, 1000, 400, 500, 24), 300)
    check("a non-zero origin is removed before mapping", Scroll.handleOffset(-20, -20, 1000, 400, 500, 24), 0)
    check("dragging the handle to the middle maps to the middle page",
          Scroll.positionForHandle(150, 0, 1000, 400, 500, 24), 300)
    check("dragging beyond the track clamps to the last page",
          Scroll.positionForHandle(900, 0, 1000, 400, 500, 24), 600)
    // A scale listing (content 100000, viewport 400, track 400) clamps its 1.6 px handle to 24, so travel is 376, not 398.4.
    check("a clamped handle starts at the top of the track", Scroll.handleOffset(0, 0, 100000, 400, 400, 24), 0)
    check("a clamped handle maps the middle page to the middle of the track", Scroll.handleOffset(49800, 0, 100000, 400, 400, 24), 188)
    check("a clamped handle maps the last page to the end of the track", Scroll.handleOffset(99600, 0, 100000, 400, 400, 24), 376)
    check("a clamped handle maps the middle of the track to the middle page", Scroll.positionForHandle(188, 0, 100000, 400, 400, 24), 49800)
    check("a clamped handle clamps a drag past the track to the last page", Scroll.positionForHandle(900, 0, 100000, 400, 400, 24), 99600)
    check("a clamped handle removes a non-zero origin before mapping", Scroll.handleOffset(49900, 100, 100000, 400, 400, 24), 188)
    check("a clamped handle adds the origin back to a dragged position", Scroll.positionForHandle(188, 100, 100000, 400, 400, 24), 49900)
}
