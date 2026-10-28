use serde::{Deserialize, Serialize};

/// Who or what requested a state-changing action.
///
/// Every mutation of the core takes an Origin so clients (and agents) can
/// tell who changed what.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Origin {
    MainWindow,
    MenuBar,
    Cli,
    /// A coding agent, through `localhost mcp`.
    Agent,
}
