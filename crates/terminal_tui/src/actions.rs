use crate::app::{ActivePanel, AppState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Focus(ActivePanel),
    StreamDisconnected,
}

pub fn apply(app: &mut AppState, action: Action) {
    match action {
        Action::Focus(panel) => app.active_panel = panel,
        Action::StreamDisconnected => app.handle_stream_disconnected(),
    }
}
