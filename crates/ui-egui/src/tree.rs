use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{Align, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use gallerybrowser_core::list_subfolders;

use crate::state::{Action, AppState, DragPaths};
use crate::theme;

const ROW_HEIGHT: f32 = 20.0;
const INDENT: f32 = 18.0;
const MAX_DEPTH: usize = 96;

/// Folder tree (lazy: children listed on expand via core::list_subfolders, cached; respects show_hidden),
/// roots: Favorites, Recent, then the platform's filesystem roots (see [`fs_roots`]);
/// auto-expands to nav.current. Accepts drops (Action::Drop). Context menu: add/remove favorite,
/// open in file manager, new folder.
#[derive(Default)]
pub struct TreeState {
    /// Expanded rows keyed by (top-level root, path): the same folder reachable from two roots
    /// (e.g. `/mnt/2Tb` under Computer and as its own mount root) expands independently.
    expanded: HashSet<(PathBuf, PathBuf)>,
    /// Lazily listed children, dropped whenever `AppState::tree_generation` changes.
    children: HashMap<PathBuf, Vec<PathBuf>>,
    exists: HashMap<PathBuf, bool>,
    roots: Vec<FsRoot>,
    generation: Option<u64>,
    last_nav: Option<PathBuf>,
    scroll_to_current: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Favorites,
    Recent,
    Filesystem,
}

/// Everything a row needs while drawing, without borrowing `AppState` (actions are applied afterwards).
struct Frame<'a> {
    tree: &'a mut TreeState,
    current: Option<PathBuf>,
    /// Folder of the inactive pane (dual-pane mode): drawn with a weaker highlight.
    other: Option<PathBuf>,
    show_hidden: bool,
    favorites: Vec<PathBuf>,
    external_drop: Vec<PathBuf>,
    acts: Vec<Action>,
    remove_recent: Option<PathBuf>,
    /// The single filesystem root that shows the current folder: the deepest root containing it.
    target_root: Option<PathBuf>,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, tree: &mut TreeState) {
    if tree.generation != Some(state.tree_generation) {
        tree.generation = Some(state.tree_generation);
        tree.children.clear();
        tree.exists.clear();
        tree.roots = fs_roots();
    }
    let target_root = state
        .pane()
        .nav
        .current
        .as_ref()
        .and_then(|cur| target_root_for(cur, &tree.roots));
    if tree.last_nav != state.pane().nav.current {
        tree.last_nav = state.pane().nav.current.clone();
        if let (Some(cur), Some(root)) = (&state.pane().nav.current, &target_root) {
            for a in cur.ancestors().skip(1).filter(|a| a.starts_with(root)) {
                tree.expanded.insert((root.clone(), a.to_path_buf()));
            }
        }
        tree.scroll_to_current = true;
    }

    let recents = state.settings.recents.clone();
    let favorites = state.settings.favorites.clone();
    let external_drop: Vec<PathBuf> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    });
    let mut f = Frame {
        current: state.pane().nav.current.clone(),
        other: if state.settings.dual_pane {
            state.pane_at(state.other_index()).nav.current.clone()
        } else {
            None
        },
        show_hidden: state.settings.show_hidden,
        favorites: favorites.clone(),
        external_drop,
        acts: Vec::new(),
        remove_recent: None,
        target_root,
        tree,
    };
    let mut clear_recents = false;

    egui::ScrollArea::both()
        .id_salt("tree_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let h = egui::CollapsingHeader::new("Favorites")
                .id_salt("tree_fav")
                .default_open(true)
                .show(ui, |ui| {
                    let list: Vec<PathBuf> =
                        favorites.iter().filter(|p| f.exists(p)).cloned().collect();
                    if list.is_empty() {
                        ui.weak("No favorites");
                    }
                    for p in list {
                        ui.push_id("fav", |ui| {
                            node(ui, &mut f, &p, &p, None, 0, Section::Favorites)
                        });
                    }
                });
            drop(h);
            let h = egui::CollapsingHeader::new("Recent")
                .id_salt("tree_recent")
                .default_open(true)
                .show(ui, |ui| {
                    let list: Vec<PathBuf> =
                        recents.iter().filter(|p| f.exists(p)).cloned().collect();
                    if list.is_empty() {
                        ui.weak("No recent folders");
                    }
                    for p in list {
                        ui.push_id(("recent", &p), |ui| {
                            node(ui, &mut f, &p, &p, None, 0, Section::Recent)
                        });
                    }
                });
            h.header_response.context_menu(|ui| {
                if ui.button("Clear Recents").clicked() {
                    clear_recents = true;
                    ui.close();
                }
            });
            ui.separator();
            ui.push_id("fs_roots", |ui| {
                let roots = f.tree.roots.clone();
                for (label, root) in roots {
                    node(ui, &mut f, &root, &root, label, 0, Section::Filesystem);
                }
            });
        });
    // A scroll request that no visible row consumed (e.g. current folder is hidden) must not linger.
    f.tree.scroll_to_current = false;

    let Frame {
        acts,
        remove_recent,
        ..
    } = f;
    state.pending.extend(acts);
    if clear_recents {
        state.settings.recents.clear();
    }
    if let Some(p) = remove_recent {
        state.settings.recents.retain(|r| *r != p);
    }
}

impl Frame<'_> {
    fn exists(&mut self, p: &Path) -> bool {
        if let Some(&e) = self.tree.exists.get(p) {
            return e;
        }
        let e = p.is_dir();
        self.tree.exists.insert(p.to_path_buf(), e);
        e
    }

    fn load_children(&mut self, p: &Path) {
        if self.tree.children.contains_key(p) {
            return;
        }
        let list = match list_subfolders(p, self.show_hidden) {
            Ok(l) => l,
            Err(e) => {
                log::debug!("tree: cannot list {}: {e}", p.display());
                Vec::new()
            }
        };
        self.tree.children.insert(p.to_path_buf(), list);
    }
}

fn node(
    ui: &mut egui::Ui,
    f: &mut Frame<'_>,
    root: &Path,
    path: &Path,
    label: Option<&str>,
    depth: usize,
    section: Section,
) {
    // Recent rows are a flat list of visited folders: never expandable, never show children.
    let flat = section == Section::Recent;
    let key = (root.to_path_buf(), path.to_path_buf());
    let expanded = !flat && f.tree.expanded.contains(&key);
    if expanded {
        f.load_children(path);
    }
    let can_expand = !flat && f.tree.children.get(path).is_none_or(|c| !c.is_empty());
    let name = label.map(str::to_string).unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
    });
    // Filesystem rows: only the target root highlights the current folder, so it opens once.
    let selected = f.current.as_deref() == Some(path)
        && (section != Section::Filesystem || f.target_root.as_deref() == Some(root));
    let other_pane = !selected && f.other.as_deref() == Some(path);
    let indent = depth as f32 * INDENT;

    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(120.0), ROW_HEIGHT),
        Sense::click(),
    );
    let drop_target = resp.dnd_hover_payload::<DragPaths>().is_some();
    if flat {
        resp.clone().on_hover_text(path.display().to_string());
    }
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 3.0, theme::ACCENT_SURFACE);
        } else if other_pane {
            p.rect_filled(rect, 3.0, theme::ACCENT_SURFACE.gamma_multiply(0.45));
        } else if resp.hovered() {
            p.rect_filled(rect, 3.0, ui.visuals().widgets.hovered.bg_fill);
        }
        if drop_target {
            p.rect_stroke(
                rect,
                3.0,
                Stroke::new(1.5, theme::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        let arrow_center = Pos2::new(rect.min.x + indent + 8.0, rect.center().y);
        if can_expand {
            let c = ui.visuals().text_color();
            let pts = if expanded {
                vec![
                    arrow_center + Vec2::new(-4.0, -2.0),
                    arrow_center + Vec2::new(4.0, -2.0),
                    arrow_center + Vec2::new(0.0, 3.0),
                ]
            } else {
                vec![
                    arrow_center + Vec2::new(-2.0, -4.0),
                    arrow_center + Vec2::new(-2.0, 4.0),
                    arrow_center + Vec2::new(3.0, 0.0),
                ]
            };
            p.add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
        }
        let icon_min = Pos2::new(rect.min.x + indent + 20.0, rect.center().y - 4.5);
        p.rect_filled(
            Rect::from_min_size(icon_min, Vec2::new(13.0, 9.0)),
            2.0,
            theme::GOLD,
        );
        let color = if selected {
            Color32::WHITE
        } else {
            ui.visuals().text_color()
        };
        p.text(
            Pos2::new(rect.min.x + indent + 38.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &name,
            egui::TextStyle::Body.resolve(ui.style()),
            color,
        );
    }

    if selected && f.tree.scroll_to_current && section == Section::Filesystem {
        resp.scroll_to_me(Some(Align::Center));
        f.tree.scroll_to_current = false;
    }

    let mut toggled = false;
    if resp.clicked() {
        let on_arrow = can_expand
            && resp
                .interact_pointer_pos()
                .is_some_and(|pos| pos.x < rect.min.x + indent + 18.0);
        if !on_arrow {
            f.acts.push(Action::Navigate(path.to_path_buf()));
        }
        // Clicking the arrow toggles only; clicking the label navigates and toggles (like the Python tree).
        // A label click on a folder that a deeper root owns (e.g. `/mnt/2Tb` under Computer) only
        // navigates: the owning root opens it, so the folder never opens twice.
        let owns = section != Section::Filesystem
            || target_root_for(path, &f.tree.roots).as_deref() == Some(root);
        toggled = can_expand && (on_arrow || owns);
    }
    if toggled {
        if expanded {
            f.tree.expanded.remove(&key);
        } else {
            f.tree.expanded.insert(key);
            f.load_children(path);
        }
    }

    if let Some(payload) = resp.dnd_release_payload::<DragPaths>() {
        let copy = ui.input(|i| i.modifiers.ctrl);
        f.acts.push(Action::Drop {
            paths: payload.0.clone(),
            dest: path.to_path_buf(),
            copy,
        });
    }
    if !f.external_drop.is_empty()
        && ui
            .input(|i| i.pointer.latest_pos())
            .is_some_and(|pos| rect.contains(pos))
    {
        let paths = std::mem::take(&mut f.external_drop);
        f.acts.push(Action::Drop {
            paths,
            dest: path.to_path_buf(),
            copy: true,
        });
    }

    resp.context_menu(|ui| {
        let is_fav = f.favorites.iter().any(|p| p == path);
        match section {
            Section::Favorites => {
                if ui.button("Remove from Favorites").clicked() {
                    f.acts.push(Action::RemoveFavorite(path.to_path_buf()));
                    ui.close();
                }
            }
            Section::Recent => {
                if !is_fav && ui.button("Add to Favorites").clicked() {
                    f.acts.push(Action::AddFavorite(path.to_path_buf()));
                    ui.close();
                }
                if ui.button("Remove from Recents").clicked() {
                    f.remove_recent = Some(path.to_path_buf());
                    ui.close();
                }
            }
            Section::Filesystem => {
                if is_fav {
                    if ui.button("Remove from Favorites").clicked() {
                        f.acts.push(Action::RemoveFavorite(path.to_path_buf()));
                        ui.close();
                    }
                } else if ui.button("Add to Favorites").clicked() {
                    f.acts.push(Action::AddFavorite(path.to_path_buf()));
                    ui.close();
                }
            }
        }
        ui.separator();
        if ui.button("Open in File Manager").clicked() {
            f.acts.push(Action::RevealInFileManager(path.to_path_buf()));
            ui.close();
        }
        if ui.button("New Folder Here…").clicked() {
            f.acts.push(Action::Navigate(path.to_path_buf()));
            f.acts.push(Action::BeginNewFolder);
            ui.close();
        }
    });

    if expanded && depth < MAX_DEPTH {
        let kids = f.tree.children.get(path).cloned().unwrap_or_default();
        for k in kids {
            node(ui, f, root, &k, None, depth + 1, section);
        }
    }
}

/// A top-level filesystem root: optional fixed label (otherwise the folder name) and its path.
type FsRoot = (Option<&'static str>, PathBuf);

/// The deepest root that is `cur` or one of its ancestors (`/mnt/2Tb` beats `/` for `/mnt/2Tb/x`).
fn target_root_for(cur: &Path, roots: &[FsRoot]) -> Option<PathBuf> {
    roots
        .iter()
        .map(|(_, r)| r)
        .filter(|r| cur.starts_with(r))
        .max_by_key(|r| r.components().count())
        .cloned()
}

/// Roots listed under the Favorites/Recent sections, in display order. Linux: `/`, Home, mounted media.
/// macOS: `/`, Home, `/Volumes/*`. Windows: Home, then every existing drive letter (no `/` root).
fn fs_roots() -> Vec<FsRoot> {
    let mut roots: Vec<FsRoot> = Vec::new();
    #[cfg(not(windows))]
    roots.push((Some("Computer"), PathBuf::from("/")));
    roots.extend(home_dir().map(|h| (Some("Home"), h)));
    roots.extend(mount_roots().into_iter().map(|m| (None, m)));
    roots
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Mounted media: non-hidden directories directly beneath /mnt, /media and /run/media/$USER.
#[cfg(all(unix, not(target_os = "macos")))]
fn mount_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut bases: Vec<PathBuf> = vec![PathBuf::from("/mnt"), PathBuf::from("/media")];
    if let Some(user) = std::env::var_os("USER") {
        bases.push(Path::new("/run/media").join(user));
    }
    for base in bases {
        if let Ok(list) = list_subfolders(&base, false) {
            roots.extend(list);
        }
    }
    roots
}

/// Mounted volumes: non-hidden directories directly beneath /Volumes.
#[cfg(target_os = "macos")]
fn mount_roots() -> Vec<PathBuf> {
    list_subfolders(Path::new("/Volumes"), false).unwrap_or_default()
}

/// Every existing drive letter, `A:\` … `Z:\`.
#[cfg(windows)]
fn mount_roots() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|letter| PathBuf::from(format!("{}:\\", char::from(letter))))
        .filter(|root| root.is_dir())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepest_root_owns_the_folder() {
        let roots: Vec<FsRoot> = vec![
            (Some("Computer"), PathBuf::from("/")),
            (Some("Home"), PathBuf::from("/home/u")),
            (None, PathBuf::from("/mnt/2Tb")),
        ];
        let r = |p: &str| target_root_for(Path::new(p), &roots);
        assert_eq!(r("/mnt/2Tb"), Some(PathBuf::from("/mnt/2Tb")));
        assert_eq!(r("/mnt/2Tb/shots"), Some(PathBuf::from("/mnt/2Tb")));
        assert_eq!(r("/mnt/2Tbx"), Some(PathBuf::from("/")));
        assert_eq!(r("/home/u/Videos"), Some(PathBuf::from("/home/u")));
        assert_eq!(r("/mnt"), Some(PathBuf::from("/")));
    }
}
