//! A close that would end a running program asks first. A shell at its prompt
//! closes without asking, as in WezTerm.

use librio::SurfaceId;
use rune::Tx;

use crate::chrome::TabGroupId;
use crate::model::Model;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Close {
    /// A pane of a tab that has others.
    Pane(SurfaceId),
    /// A tab and every pane in it, by one of its surfaces: a dialog can be open
    /// while the strip changes, so an index could name another tab by then.
    Tab(SurfaceId),
    /// A session's tab, hidden, which ends nothing.
    Hide {
        group: TabGroupId,
        window: u32,
    },
    SessionPane {
        group: TabGroupId,
        pane: u32,
    },
    SessionWindow {
        group: TabGroupId,
        window: u32,
    },
}

/// A close waiting on the user, and the programs it would end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Closing {
    pub close: Close,
    pub running: Vec<String>,
}

/// Closing tab `index` as a whole.
pub(crate) fn tab(model: &Model, index: usize) -> Option<Close> {
    let tab = model.tabs.get(index)?;
    match (tab.group, tab.window) {
        (Some(group), Some(window)) => Some(Close::Hide { group, window }),
        _ => crate::tabs::surfaces_of(&tab.root)
            .first()
            .copied()
            .map(Close::Tab),
    }
}

fn index_of(model: &Model, close: Close) -> Option<usize> {
    model.tabs.iter().position(|tab| match close {
        Close::Tab(id) => crate::tabs::surfaces_of(&tab.root).contains(&id),
        Close::Hide { group, window } => tab.group == Some(group) && tab.window == Some(window),
        _ => false,
    })
}

pub(crate) fn request(model: &mut Model, close: Close, tx: &mut Tx) {
    if !model.config.settings.confirm_close {
        perform(model, close, tx);
        return;
    }
    let running = match close {
        Close::Pane(id) => model.shared.running(id).into_iter().collect(),
        Close::Tab(_) => match index_of(model, close).map(|index| &model.tabs[index]) {
            Some(tab) => crate::tabs::surfaces_of(&tab.root)
                .into_iter()
                .filter_map(|id| model.shared.running(id))
                .collect(),
            None => Vec::new(),
        },
        Close::Hide { .. } => Vec::new(),
        Close::SessionPane { .. } | Close::SessionWindow { .. } => {
            crate::mux::ask_running(model, close);
            return;
        }
    };
    answered(model, close, running, tx);
}

/// What `close` would end, which a session's multiplexer answers later.
pub(crate) fn answered(model: &mut Model, close: Close, running: Vec<String>, tx: &mut Tx) -> bool {
    if running.is_empty() {
        perform(model, close, tx);
    } else {
        model.closing = Some(Closing { close, running });
    }
    true
}

pub(crate) fn confirm(model: &mut Model, tx: &mut Tx) {
    if let Some(closing) = model.closing.take() {
        perform(model, closing.close, tx);
    }
}

fn perform(model: &mut Model, close: Close, tx: &mut Tx) {
    match close {
        Close::Pane(id) => {
            crate::delegate::drop_closed(model, id, tx);
        }
        Close::Tab(_) | Close::Hide { .. } => {
            if let Some(index) = index_of(model, close) {
                crate::tabs::close(model, index, tx);
            }
        }
        Close::SessionPane { .. } | Close::SessionWindow { .. } => crate::mux::kill(model, close),
    }
}
