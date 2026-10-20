use std::path::{Component, PathBuf};

use gallerybrowser_core::FilterScope;

use crate::state::{Action, AppState};
use crate::theme;

/// Clickable path segments of `state.nav.current` + back/forward/up buttons; click pushes Action::Navigate.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!state.nav.back.is_empty(), egui::Button::new("◀"))
            .on_hover_text("Back (Alt+Left)")
            .clicked()
        {
            state.push(Action::Back);
        }
        if ui
            .add_enabled(!state.nav.forward.is_empty(), egui::Button::new("▶"))
            .on_hover_text("Forward (Alt+Right)")
            .clicked()
        {
            state.push(Action::Forward);
        }
        let has_parent = state
            .nav
            .current
            .as_ref()
            .is_some_and(|p| p.parent().is_some());
        if ui
            .add_enabled(has_parent, egui::Button::new("▲"))
            .on_hover_text("Parent folder (Backspace)")
            .clicked()
        {
            state.push(Action::Up);
        }
        if ui.button("⟳").on_hover_text("Refresh (F5)").clicked() {
            state.push(Action::Refresh);
        }
        ui.separator();
        ui.label(egui::RichText::new("Path").color(theme::TEXT_MUTED));

        match (&state.filters.scope, &state.nav.current) {
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
                            for (i, (label, target)) in segments.into_iter().enumerate() {
                                let text = elide(&label, 28);
                                let mut rt = egui::RichText::new(text);
                                if i == last {
                                    rt = rt.strong();
                                }
                                let resp = ui
                                    .add(egui::Button::new(rt).frame(false))
                                    .on_hover_text(target.display().to_string());
                                if resp.clicked() {
                                    state.push(Action::Navigate(target));
                                }
                                if i != last {
                                    ui.label(egui::RichText::new("›").color(theme::TEXT_MUTED));
                                }
                            }
                        });
                    });
            }
        }
    });
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
