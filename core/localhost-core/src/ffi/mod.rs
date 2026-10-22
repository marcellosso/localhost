//! UniFFI bindings for the macOS app, built with the `ffi` feature.
//!
//! The data types derive their UniFFI traits where they are defined. This
//! module adds what the derives can't express: custom types, and the objects
//! the app holds (the library, the manager, the log store, subscriptions).

mod functions;
mod objects;
mod runtime;
mod subscriptions;

use std::collections::{BTreeMap, HashMap};

use crate::{Id, Key};

uniffi::custom_type!(Id, String, {
    lower: |id| id.to_string(),
    try_lift: |raw| Ok(raw.parse()?),
});

uniffi::custom_type!(Key, String, {
    lower: |key| key.as_str().to_string(),
    try_lift: |raw| Ok(Key::new(raw)?),
});

/// Environment variables and other ordered string maps: a Swift dictionary.
type StringMap = BTreeMap<String, String>;

uniffi::custom_type!(StringMap, HashMap<String, String>, {
    remote,
    lower: |values| values.into_iter().collect(),
    try_lift: |values| Ok(values.into_iter().collect()),
});

uniffi::custom_type!(usize, u64, {
    remote,
    lower: |value| value as u64,
    try_lift: |value| Ok(usize::try_from(value)?),
});
