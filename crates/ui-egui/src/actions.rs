//! Central action dispatcher and the single refresh pipeline. (Shell slice.)

use crate::state::{Action, AppState};

/// Apply every queued action in order (actions may enqueue more; bounded to avoid loops).
pub fn dispatch_pending(state: &mut AppState, ctx: &egui::Context) {
    let actions = std::mem::take(&mut state.pending);
    for a in actions {
        dispatch(state, ctx, a);
    }
}

pub fn dispatch(_state: &mut AppState, _ctx: &egui::Context, _action: Action) {}

/// Rescan (if `rescan`) and rebuild `state.entries`; reconcile selection; request thumbnails.
pub fn refresh_view(_state: &mut AppState, _rescan: bool) {}

/// Drain fileops / thumbnail / metadata worker events once per frame (relink library on moves, etc.).
pub fn poll_workers(_state: &mut AppState, _ctx: &egui::Context) {}
