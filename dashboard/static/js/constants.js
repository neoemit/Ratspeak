// Centralized values used across dashboard modules.

var RS = window.RS || {};

RS.config = {
    TOAST_DURATION: 3000,
    TOAST_ERROR_DURATION: 5000,
    TOAST_CRITICAL_DURATION: 8000,
    TOAST_FADE_MS: 350,

    CONNECTIONS_THROTTLE: 5000,
    CONTACT_STATUS_POLL: 30000,
    CONNECTION_TIMEOUT: 30000,

    DEBOUNCE_SEARCH: 150,
    DEBOUNCE_RESIZE: 200,

    TRANSITION_FALLBACK: 350,

    SERVER_RESTART_DELAY: 6000,

    CONNECTION_SUMMARY_DELAY: 10000,

    // Seconds, not ms.
    STATUS_EVENT_THROTTLE_S: 280,

    FIRST_RUN_TOOLTIP_DELAY: 2000,

    MAX_EVENTS: 200,
    MAX_ANNOUNCES: 200,
    MAX_ANNOUNCES_DISPLAY: 50,
    MAX_INTERFACE_HISTORY: 60,

    MOBILE_BREAKPOINT: 768,
    MOBILE_TOUCH_BREAKPOINT: 1024,
};

// Gesture thresholds consumed by gestures.js / view_stack.js.
RS.gestures = {
    EDGE_ZONE_PX: 40,
    EDGE_MARGIN_TAB_SWIPE_PX: 30,
    SWIPE_VELOCITY_PX_MS: 0.3,
    SWIPE_DISTANCE_PX: 60,
    SWIPE_DISTANCE_DRILLBACK_PX: 80,
    SWIPE_DISTANCE_CONV_DELETE_PX: 100,
    SWIPE_DISTANCE_TOAST_DISMISS_PX: 40,
    SWIPE_DWELL_SIDEBAR_OPEN_MS: 200,
    LONG_PRESS_BOTTOM_BAR_MS: 1350,
    LONG_PRESS_BOTTOM_BAR_DELAY_MS: 150,
    LONG_PRESS_GAMES_ROW_MS: 500,
    LONG_PRESS_SEND_MS: 500,
    LONG_PRESS_MOVE_CANCEL_PX: 20,
    DRAG_DISMISS_THRESHOLD_PX: 60,
    DRAG_DISMISS_OPACITY_DENOM_PX: 300,
    PULL_TO_REFRESH_DISTANCE_PX: 60,
    PULL_TO_REFRESH_RUBBER_BAND_FACTOR: 3,
    PULL_TO_REFRESH_MIN_MS: 600,
    PULL_TO_REFRESH_SUCCESS_MS: 300,
    RIPPLE_DURATION_MS: 400,
    RIPPLE_SELECTORS: [
        '.bottom-bar-item', '.bottom-sheet-item', '.nav-item',
        '.nr-btn', '.conv-row', '.contacts-row', '.conn-row', '.conn-card'
    ],
    RIPPLE_HAPTIC_SELECTORS: [
        '.bottom-bar-item', '.bottom-sheet-item', '.nav-item', '.nr-btn'
    ],
    DRILL_DOWN_VIEWS: ['identity', 'network', 'settings', 'eventlog', 'propagation'],
    HAPTIC_DURATION_MAP: { light: 10, medium: 20, heavy: 30, success: 15, warning: 25, error: 40, selection: 8 }
};
