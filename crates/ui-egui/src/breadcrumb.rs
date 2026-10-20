use std::path::{Component, PathBuf};

use gallerybrowser_core::FilterScope;

use crate::state::{Action, AppState};
use crate::theme;

/// Clickable path segments of `state.pane().nav.current` + back/forward/up buttons; click pushes Action::Navigate.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let has_back = !state.pane().nav.back.is_empty();
        let has_forward = !state.pane().nav.forward.is_empty();
        let has_parent = state
            .pane()
            .nav
            .current
            .as_ref()
            .is_some_and(|p| p.parent().is_some());
        ui.spacing_mut().item_spacing.x = 4.0;
        if nav_button(ui, has_back, theme::Icon::Back, "Back", "Back (Alt+Left)") {
            state.push(Action::Back);
        }
        if nav_button(
            ui,
            has_forward,
            theme::Icon::Forward,
            "Forward",
            "Forward (Alt+Right)",
        ) {
            state.push(Action::Forward);
        }
        if nav_button(
            ui,
            has_parent,
            theme::Icon::Up,
            "Parent folder",
            "Parent folder (Backspace)",
        ) {
            state.push(Action::Up);
        }
        if nav_button(ui, true, theme::Icon::Refresh, "Refresh", "Refresh (F5)") {
            state.push(Action::Refresh);
        }
        ui.spacing_mut().item_spacing.x = 6.0;
        theme::group_separator(ui);

        match (&state.pane().filters.scope, &state.pane().nav.current) {
            (FilterScope::Library, _) => {
                ui.label(egui::RichText::new("Library").strong());
            }
            (_, None) => {
                ui.label(egui::RichText::new("No folder open").color(theme::TEXT_MUTED));
            }
            (_, Some(current)) => {
                let segments = segments(current);
                let last = segments.len().saturating_sub(1);
                egui::ScrollArea::horizontal()
                    .id_salt("breadcrumb_scroll")
                    .auto_shrink([false, true])
                    .stick_to_right(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                            for (i, (label, target)) in segments.into_iter().enumerate() {
                                let text = elide(&label, 28);
                                let resp = ui
                                    .add(egui::Button::selectable(i == last, text))
                                    .on_hover_text(target.display().to_string());
                                if resp.clicked() {
                                    state.push(Action::Navigate(target));
                                }
                                if i != last {
                                    theme::path_separator(ui);
                                }
                            }
                        });
                    });
            }
        }
    });
}

/// Icon button that is greyed out and inert when `enabled` is false; true when clicked.
fn nav_button(ui: &mut egui::Ui, enabled: bool, icon: theme::Icon, label: &str, tip: &str) -> bool {
    ui.add_enabled_ui(enabled, |ui| {
        theme::icon_button(ui, icon, label)
            .on_hover_text(tip)
            .on_disabled_hover_text(tip)
            .clicked()
    })
    .inner
}

/// (label, accumulated absolute path) for the root and every component.
fn segments(path: &std::path::Path) -> Vec<(String, PathBuf)> {
    let mut acc = PathBuf::new();
    let mut out = Vec::new();
    for c in path.components() {
        acc.push(c.as_os_str());
        let label = match c {
            Component::RootDir => "/".to_string(),
            other => other.as_os_str().to_string_lossy().into_owned(),
        };
        out.push((label, acc.clone()));
    }
    out
}

/// Middle-elide to at most `max` characters.
fn elide(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max < 5 {
        return s.to_string();
    }
    let keep = (max - 1) / 2;
    let head: String = s.chars().take(keep).collect();
    let tail: String = s.chars().skip(n - keep).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elides_middle() {
        assert_eq!(elide("short", 28), "short");
        let e = elide(&"a".repeat(40), 11);
        assert_eq!(e.chars().count(), 11);
        assert!(e.contains('…'));
    }

    #[test]
    fn segments_accumulate() {
        let s = segments(std::path::Path::new("/a/b"));
        assert_eq!(s.len(), 3);
        assert_eq!(s[2].1, PathBuf::from("/a/b"));
        assert_eq!(s[0].0, "/");
    }
}
