.pragma library

// The listing swap's decisions, see AGENTS.md "The listing swap"; ui/PaneSwap.qml holds the state and the timer.

// The loading mark's own hold-off, ui/LoadingState.qml: a listing slower than this shows the mark when it always did.
var HOLD_MS = 150

// The listing keys still answered while a listing is out: the navigations refuse themselves with the
// loading sentence, escape touches no row, and the rest change the window rather than a file.
var ANSWERED_WHILE_LISTING = ["open", "parent", "historyBack", "historyForward", "escape",
                              "viewList", "viewColumns", "viewGrid", "sidebar", "windowNew", "togglePreview"]

// What a swallowed key says, the sentence every refused navigation already gives.
var LOADING = "A directory is already loading."

function idle() {
    return { holding: false, listed: null, fellBack: false, walk: false, query: "", since: 0 }
}

// Only a settled directory listing is held: a walk's matches or another view's rows would show half of each.
function begin(state, listingState, searchMode, ask, now) {
    var holding = ask.clearAtOnce !== true && listingState !== "loading" && searchMode === "" && state.walk !== true
    return { holding: holding, listed: null, fellBack: false, walk: state.walk, query: ask.keptQuery || "", since: now }
}

// The first listed line after the request is the one the held rows are waiting for.
function keeps(state) {
    return state.holding && state.listed === null
}

function kept(state, listed) {
    return Object.assign({}, state, { listed: listed })
}

// Every listed line that is applied rather than kept says whether the rows it brings are a walk's.
function heard(state, walk) {
    return Object.assign({}, state, { walk: walk === true })
}

// A held listing is only ever a directory's, which is what begin() asked for.
function landed(state) {
    return Object.assign({}, state, { holding: false, listed: null, fellBack: false, walk: false })
}

// The cap ran out: the pane shows the loading state until this listing ends.
function expired(state) {
    return Object.assign({}, state, { holding: false, listed: null, fellBack: true })
}

// A failed or refused listing ends the hold without a swap; the failure draws what it always did.
function dropped(state) {
    return Object.assign({}, state, { holding: false, listed: null, fellBack: false })
}

// The listing is no longer out, however it ended.
function ended(state) {
    return Object.assign({}, state, { holding: false, listed: null, fellBack: false })
}

// A key swallowed while a listing is out, held or loading: the backend already numbers rows for the directory asked for.
function swallows(inFlight, action) {
    return inFlight === true && action.length > 0 && ANSWERED_WHILE_LISTING.indexOf(action) < 0
}

// A frame drawn empty while a listing is out: "blank" before the cap is the defect, "loading" after it is allowed.
function frameKind(inFlight, listingState, fellBack) {
    if (inFlight !== true || listingState !== "loading") {
        return ""
    }
    return fellBack === true ? "loading" : "blank"
}
