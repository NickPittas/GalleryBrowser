use std::path::{Component, Path, PathBuf};

use gallerybrowser_core::FilterScope;

use crate::state::{Action, AppState};
use crate::theme;

/// Per-pane path editor state, kept in egui memory keyed by pane index.
#[derive(Clone, Default)]
struct EditState {
    text: String,
    /// First frame of editing: grab focus and select all.
    fresh: bool,
}

fn edit_id(pane: usize) -> egui::Id {
    egui::Id::new(("breadcrumb_edit", pane))
}

/// Outcome of resolving text typed into the path editor.
#[derive(Debug, PartialEq, Eq)]
enum Resolved {
    Folder(PathBuf),
    File { file: PathBuf, parent: PathBuf },
}

/// Resolve user-typed path text: trim, expand a leading `~`, resolve relative paths against `base`,
/// drop trailing separators; folders navigate, files navigate to their parent.
fn resolve_input(text: &str, home: Option<&Path>, base: Option<&Path>) -> Result<Resolved, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("Path is empty".to_string());
    }
    let expanded: PathBuf = if t == "~" {
        home.map(Path::to_path_buf)
            .ok_or_else(|| "Home folder is unknown".to_string())?
    } else if let Some(rest) = t.strip_prefix("~/").or_else(|| t.strip_prefix("~\\")) {
        home.ok_or_else(|| "Home folder is unknown".to_string())?
            .join(rest)
    } else {
        PathBuf::from(t)
    };
    let absolute = match base {
        Some(b) if expanded.is_relative() => b.join(expanded),
        _ => expanded,
    };
    // `components()` drops trailing separators and `.` segments.
    let path: PathBuf = absolute.components().collect();
    if path.is_dir() {
        Ok(Resolved::Folder(path))
    } else if path.is_file() {
        match path.parent() {
            Some(parent) => Ok(Resolved::File {
                parent: parent.to_path_buf(),
                file: path,
            }),
            None => Err(format!("Cannot open {}", path.display())),
        }
    } else {
        Err(format!("No such file or folder: {}", path.display()))
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Clickable path segments of `state.pane().nav.current` + back/forward/up buttons; click pushes Action::Navigate.
/// `state.active` is the pane being drawn; `is_active` tells whether it is the app's real active pane
/// (Ctrl+L only edits that one). Copy / edit-path buttons follow the segments.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, is_active: bool) {
    let pane_index = state.active;
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

        let id = edit_id(pane_index);
        let folder = match (&state.pane().filters.scope, &state.pane().nav.current) {
            (FilterScope::Library, _) => {
                ui.label(egui::RichText::new("Library").strong());
                return;
            }
            (_, None) => {
                ui.label(egui::RichText::new("No folder open").color(theme::TEXT_MUTED));
                return;
            }
            (_, Some(current)) => current.clone(),
        };

        let ctx = ui.ctx().clone();
        let mut editing: Option<EditState> = ctx.data_mut(|d| d.get_temp(id));
        let mut start_edit = false;
        if is_active
            && editing.is_none()
            && !ctx.egui_wants_keyboard_input()
            && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::L))
        {
            start_edit = true;
        }

        if let Some(edit) = editing.as_mut() {
            let mut keep = true;
            let out = egui::TextEdit::singleline(&mut edit.text)
                .id_salt(("breadcrumb_edit_text", pane_index))
                .desired_width(f32::INFINITY)
                .show(ui);
            let resp = out.response;
            if edit.fresh {
                resp.request_focus();
                let mut st = out.state;
                let len = edit.text.chars().count();
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(len),
                )));
                st.store(&ctx, resp.id);
                edit.fresh = false;
            } else if resp.lost_focus() {
                let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                if enter {
                    match resolve_input(&edit.text, home_dir().as_deref(), Some(&folder)) {
                        Ok(r) => {
                            state.push(Action::FocusPane(pane_index));
                            match r {
                                Resolved::Folder(p) => state.push(Action::Navigate(p)),
                                Resolved::File { file, parent } => {
                                    state.pane_mut().pending_select = Some(file);
                                    state.push(Action::Navigate(parent));
                                }
                            }
                            keep = false;
                        }
                        Err(msg) => {
                            state.error(msg);
                            resp.request_focus();
                        }
                    }
                } else {
                    keep = false;
                }
            }
            if keep {
                ctx.data_mut(|d| d.insert_temp(id, edit.clone()));
            } else {
                ctx.data_mut(|d| d.remove_temp::<EditState>(id));
            }
            return;
        }

        // Empty breadcrumb space: double-click enters edit mode (registered first so segment buttons stay on top).
        let area = ui.available_rect_before_wrap();
        let bg = ui.interact(area, id.with("bg"), egui::Sense::click());
        if bg.double_clicked() {
            start_edit = true;
        }

        let segments = segments(&folder);
        let last = segments.len().saturating_sub(1);
        let mut copy = false;
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
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if nav_button(ui, true, theme::Icon::Copy, "Copy path", "Copy path") {
                        copy = true;
                    }
                    if nav_button(
                        ui,
                        true,
                        theme::Icon::Edit,
                        "Edit path",
                        "Edit path (Ctrl+L)",
                    ) {
                        start_edit = true;
                    }
                });
            });
        if copy {
            ctx.copy_text(folder.display().to_string());
            state.info("Copied path");
        }
        if start_edit {
            let text = folder.display().to_string();
            ctx.data_mut(|d| {
                d.insert_temp(id, EditState { text, fresh: true });
            });
            ctx.request_repaint();
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

    #[test]
    fn resolves_folder_tilde_and_trailing_slash() -> std::io::Result<()> {
        let home = tempfile::tempdir()?;
        std::fs::create_dir(home.path().join("pics"))?;
        let h = Some(home.path());
        let want = Ok(Resolved::Folder(home.path().join("pics")));
        assert_eq!(resolve_input("~/pics", h, None), want);
        assert_eq!(resolve_input("  ~/pics///  ", h, None), want);
        assert_eq!(
            resolve_input("~", h, None),
            Ok(Resolved::Folder(home.path().to_path_buf()))
        );
        assert_eq!(resolve_input("pics/", None, h), want);
        assert!(resolve_input("~/pics", None, None).is_err());
        Ok(())
    }

    #[test]
    fn file_resolves_to_parent() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let f = dir.path().join("a.txt");
        std::fs::write(&f, b"x")?;
        let got = resolve_input(&f.display().to_string(), None, None);
        assert_eq!(
            got,
            Ok(Resolved::File {
                file: f,
                parent: dir.path().to_path_buf()
            })
        );
        Ok(())
    }

    #[test]
    fn nonexistent_and_empty_rejected() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let missing = dir.path().join("nope");
        assert!(resolve_input(&missing.display().to_string(), None, None).is_err());
        assert!(resolve_input("   ", None, None).is_err());
        Ok(())
    }
}
