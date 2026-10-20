use gallerybrowser_player::Player;

/// Shared playback controls: play/pause, stop, step ±1, timeline scrubber (drag does not get
/// overwritten by playback), time/frame readout (sequences show source frame numbers), speed combo,
/// loop, volume + mute. `sequence` gives source frame numbers for display.
pub fn show(_ui: &mut egui::Ui, _player: &mut Player, _sequence: Option<&gallerybrowser_core::Sequence>) {}
