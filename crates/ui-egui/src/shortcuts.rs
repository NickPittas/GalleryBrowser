use egui::{Event, Key, Modifiers};

use crate::state::{Action, AppState};

/// Global keyboard shortcuts (when no text field has focus): see docs/user_guide/KEYBOARD_SHORTCUTS.md.
/// Arrow keys, Home/End and Enter belong to the file pane; the fullscreen viewport handles its own keys.
pub fn handle(ctx: &egui::Context, state: &mut AppState) {
    if state.fullscreen.is_some() || ctx.egui_wants_keyboard_input() {
        return;
    }
    let d = &state.dialogs;
    let dialog_open = d.settings.is_some()
        || d.batch_rename.is_some()
        || d.rename.is_some()
        || d.new_folder.is_some()
        || d.confirm_delete.is_some()
        || d.about
        || d.shortcuts
        || d.new_tag.is_some()
        || d.new_collection.is_some();
    if dialog_open {
        return;
    }

    let ctrl = Modifiers::COMMAND;
    let ctrl_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    let mut out: Vec<Action> = Vec::new();
    let mut step = 0i32;
    let mut toggle_play = false;
    ctx.input_mut(|i| {
        // egui turns Ctrl+C/X/V into clipboard events rather than key presses.
        let mut keep = Vec::with_capacity(i.events.len());
        for ev in std::mem::take(&mut i.events) {
            match ev {
                Event::Copy => out.push(Action::Copy),
                Event::Cut => out.push(Action::Cut),
                Event::Paste(_) => out.push(Action::Paste { dest: None }),
                other => keep.push(other),
            }
        }
        i.events = keep;

        if i.consume_key(ctrl_shift, Key::N) {
            out.push(Action::BeginNewFolder);
        }
        if i.consume_key(ctrl_shift, Key::Z) {
            out.push(Action::Redo);
        }
        if i.consume_key(ctrl, Key::Z) {
            out.push(Action::Undo);
        }
        if i.consume_key(ctrl, Key::Y) {
            out.push(Action::Redo);
        }
        if i.consume_key(ctrl, Key::O) {
            out.push(Action::OpenFolderDialog);
        }
        if i.consume_key(ctrl, Key::Q) {
            out.push(Action::Quit);
        }
        if i.consume_key(ctrl, Key::A) {
            out.push(Action::SelectAll);
        }
        if i.consume_key(ctrl, Key::P) {
            out.push(Action::TogglePreviewPane);
        }
        if i.consume_key(Modifiers::SHIFT, Key::Delete) {
            out.push(Action::RequestDeletePermanently);
        }
        if i.consume_key(Modifiers::NONE, Key::Delete) {
            out.push(Action::Trash);
        }
        if i.consume_key(Modifiers::NONE, Key::F2) {
            out.push(Action::BeginRename);
        }
        if i.consume_key(Modifiers::NONE, Key::F5) {
            out.push(Action::Refresh);
        }
        if i.consume_key(Modifiers::NONE, Key::F1) {
            out.push(Action::ShowShortcuts);
        }
        if i.consume_key(Modifiers::NONE, Key::Escape) {
            out.push(Action::ClearSelection);
        }
        if i.consume_key(Modifiers::NONE, Key::Backspace) {
            out.push(Action::Up);
        }
        if i.consume_key(Modifiers::ALT, Key::ArrowLeft) {
            out.push(Action::Back);
        }
        if i.consume_key(Modifiers::ALT, Key::ArrowRight) {
            out.push(Action::Forward);
        }
        if i.consume_key(Modifiers::NONE, Key::Comma) {
            step -= 1;
        }
        if i.consume_key(Modifiers::NONE, Key::Period) {
            step += 1;
        }
        if i.consume_key(Modifiers::NONE, Key::Space) {
            toggle_play = true;
        }
    });
    if step != 0 {
        state.preview.step_frame(step);
    }
    if toggle_play {
        state.preview.toggle_play();
    }
    state.pending.extend(out);
}

/// (keys, description) rows for the shortcuts dialog.
pub fn table() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Ctrl+O", "Open folder"),
        ("Ctrl+Shift+N", "New folder"),
        ("Backspace", "Go to parent folder"),
        ("Alt+Left / Alt+Right", "Back / forward in folder history"),
        ("F5", "Refresh current view"),
        ("Esc", "Clear file selection"),
        ("Ctrl+A", "Select all visible items"),
        ("Ctrl+Click", "Toggle item selection"),
        ("Shift+Click", "Select a range"),
        (
            "Arrows, Home, End",
            "Move focus (Shift extends the selection)",
        ),
        ("Enter", "Open folder / fullscreen viewer"),
        ("Ctrl+C / Ctrl+X / Ctrl+V", "Copy / cut / paste"),
        ("Delete", "Move selected items to the trash"),
        ("Shift+Delete", "Delete selected items permanently"),
        ("F2", "Rename selected item"),
        ("Ctrl+Z / Ctrl+Y", "Undo / redo file operation"),
        (", / .", "Step backward / forward one frame in the preview"),
        ("Space", "Play / pause the preview"),
        ("Ctrl+P", "Show / hide the preview pane"),
        ("F1", "Keyboard shortcuts"),
        ("Ctrl+Q", "Quit"),
    ]
}
