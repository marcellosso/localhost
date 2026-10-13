use serde::{Deserialize, Serialize};

/// Who or what requested a state-changing action.
///
/// Every mutation of the core takes an Origin so clients (and later agents)
/// can tell who changed what.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    MainWindow,
    MenuBar,
    Cli,
}
