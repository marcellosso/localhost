use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::Id;

/// Where a line of a Run's output came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum OutputChannel {
    Stdout,
    Stderr,
    /// Written by localhost itself, e.g. how the Run ended.
    Localhost,
}

impl OutputChannel {
    /// The name used in log files: `stdout`, `stderr` or `localhost`.
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputChannel::Stdout => "stdout",
            OutputChannel::Stderr => "stderr",
            OutputChannel::Localhost => "localhost",
        }
    }
}

/// One line of a Run's output.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OutputLine {
    pub channel: OutputChannel,
    /// The line without its terminator. Invalid UTF-8 is replaced with U+FFFD.
    pub text: String,
    #[cfg_attr(feature = "specta", specta(type = crate::SerdeSystemTime))]
    pub timestamp: SystemTime,
}

impl OutputLine {
    pub fn new(channel: OutputChannel, text: impl Into<String>) -> OutputLine {
        OutputLine { channel, text: text.into(), timestamp: SystemTime::now() }
    }
}

/// A line of one Service's output, in a stream that combines several Services.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ServiceOutputLine {
    pub service_id: Id,
    pub line: OutputLine,
}
