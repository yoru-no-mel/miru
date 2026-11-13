use crate::filters::FilterMode;
use egui::{Key, Modifiers};

/// Actions triggered by keyboard shortcuts (UI-SPEC table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    Prev,
    Next,
    First,
    Last,
    ZoomIn,
    ZoomOut,
    FitWindow,
    FitWidth,
    FitHeight,
    ActualSize,
    RotateCw,
    RotateCcw,
    ToggleLock,
    ToggleFullscreen,
    OpenFile,
    OpenFolder,
    ClosePopoverOrFullscreen,
    /// Arrow Up: scroll up like the wheel (pan / prev at the top edge).
    ScrollUp,
    /// Arrow Down: scroll down like the wheel (pan / next at the bottom).
    ScrollDown,
    /// `[`: open the previous file/folder/archive sibling of the source.
    PrevSource,
    /// `]`: open the next file/folder/archive sibling of the source.
    NextSource,
    /// U / I / S: switch the image scaling filter.
    Filter(FilterMode),
    /// ⌘Q (macOS) / Ctrl+Q: quit the app.
    Close,
}

/// Match a key press to an action. Called for each pressed key.
pub fn match_shortcut(key: Key, modifiers: Modifiers) -> Option<ShortcutAction> {
    // Platform-appropriate accelerator: Cmd on macOS, Ctrl elsewhere.
    let accel = modifiers.command;
    let shift = modifiers.shift;
    match (key, accel, shift) {
        (Key::ArrowLeft, false, _) => Some(ShortcutAction::Prev),
        (Key::Comma, false, _) => Some(ShortcutAction::Prev),
        (Key::ArrowRight, false, _) => Some(ShortcutAction::Next),
        (Key::Period, false, _) => Some(ShortcutAction::Next),
        (Key::ArrowUp, false, _) => Some(ShortcutAction::ScrollUp),
        (Key::ArrowDown, false, _) => Some(ShortcutAction::ScrollDown),
        (Key::OpenBracket, false, _) => Some(ShortcutAction::PrevSource),
        (Key::CloseBracket, false, _) => Some(ShortcutAction::NextSource),
        (Key::Home, false, _) => Some(ShortcutAction::First),
        (Key::End, false, _) => Some(ShortcutAction::Last),
        (Key::Plus, false, _) => Some(ShortcutAction::ZoomIn),
        (Key::Equals, false, _) => Some(ShortcutAction::ZoomIn),
        (Key::Minus, false, _) => Some(ShortcutAction::ZoomOut),
        (Key::Num0, false, _) => Some(ShortcutAction::FitHeight),
        (Key::Num1, false, _) => Some(ShortcutAction::Filter(FilterMode::NoFilter)),
        (Key::Num2, false, _) => Some(ShortcutAction::Filter(FilterMode::Soften)),
        (Key::Num3, false, _) => Some(ShortcutAction::Filter(FilterMode::SoftenSharpen)),
        (Key::Num7, false, _) => Some(ShortcutAction::ActualSize),
        (Key::Num8, false, _) => Some(ShortcutAction::FitWindow),
        (Key::Num9, false, _) => Some(ShortcutAction::FitWidth),
        (Key::R, false, true) => Some(ShortcutAction::RotateCcw),
        (Key::R, false, false) => Some(ShortcutAction::RotateCw),
        (Key::L, false, _) => Some(ShortcutAction::ToggleLock),
        (Key::F11, _, _) => Some(ShortcutAction::ToggleFullscreen),
        (Key::O, true, false) => Some(ShortcutAction::OpenFile),
        (Key::O, true, true) => Some(ShortcutAction::OpenFolder),
        (Key::Q, true, false) => Some(ShortcutAction::Close),
        (Key::Escape, false, _) => Some(ShortcutAction::ClosePopoverOrFullscreen),
        _ => None,
    }
}

/// Drain pressed keys from egui and map to actions.
pub fn drain_shortcuts(ctx: &egui::Context) -> Vec<ShortcutAction> {
    let mut out = Vec::new();
    ctx.input(|i| {
        for ev in &i.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = ev
            {
                if let Some(a) = match_shortcut(*key, *modifiers) {
                    out.push(a);
                }
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(ctrl: bool, shift: bool) -> Modifiers {
        Modifiers {
            ctrl,
            shift,
            // Platform-agnostic: egui sets `command` for Cmd on macOS and
            // for Ctrl elsewhere; match_shortcut only reads `command`.
            command: ctrl,
            ..Default::default()
        }
    }

    #[test]
    fn shortcut_table() {
        assert_eq!(
            match_shortcut(Key::ArrowLeft, mods(false, false)),
            Some(ShortcutAction::Prev)
        );
        assert_eq!(
            match_shortcut(Key::ArrowRight, mods(false, false)),
            Some(ShortcutAction::Next)
        );
        assert_eq!(
            match_shortcut(Key::ArrowUp, mods(false, false)),
            Some(ShortcutAction::ScrollUp)
        );
        assert_eq!(
            match_shortcut(Key::ArrowDown, mods(false, true)),
            Some(ShortcutAction::ScrollDown)
        );
        assert_eq!(
            match_shortcut(Key::OpenBracket, mods(false, false)),
            Some(ShortcutAction::PrevSource)
        );
        assert_eq!(
            match_shortcut(Key::CloseBracket, mods(false, false)),
            Some(ShortcutAction::NextSource)
        );
        assert_eq!(
            match_shortcut(Key::R, mods(false, false)),
            Some(ShortcutAction::RotateCw)
        );
        assert_eq!(
            match_shortcut(Key::R, mods(false, true)),
            Some(ShortcutAction::RotateCcw)
        );
        assert_eq!(
            match_shortcut(Key::O, mods(true, false)),
            Some(ShortcutAction::OpenFile)
        );
        assert_eq!(
            match_shortcut(Key::O, mods(true, true)),
            Some(ShortcutAction::OpenFolder)
        );
        assert_eq!(
            match_shortcut(Key::L, mods(false, false)),
            Some(ShortcutAction::ToggleLock)
        );
        assert_eq!(
            match_shortcut(Key::Q, mods(true, false)),
            Some(ShortcutAction::Close)
        );
        assert_eq!(
            match_shortcut(Key::Num1, mods(false, false)),
            Some(ShortcutAction::Filter(FilterMode::NoFilter))
        );
        assert_eq!(
            match_shortcut(Key::Num2, mods(false, false)),
            Some(ShortcutAction::Filter(FilterMode::Soften))
        );
        assert_eq!(
            match_shortcut(Key::Num3, mods(false, false)),
            Some(ShortcutAction::Filter(FilterMode::SoftenSharpen))
        );
        assert_eq!(
            match_shortcut(Key::Num7, mods(false, false)),
            Some(ShortcutAction::ActualSize)
        );
        assert_eq!(
            match_shortcut(Key::Num8, mods(false, false)),
            Some(ShortcutAction::FitWindow)
        );
        assert_eq!(
            match_shortcut(Key::Num9, mods(false, false)),
            Some(ShortcutAction::FitWidth)
        );
        assert_eq!(
            match_shortcut(Key::Num0, mods(false, false)),
            Some(ShortcutAction::FitHeight)
        );
        assert_eq!(match_shortcut(Key::A, mods(false, false)), None);
    }
}
