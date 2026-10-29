//! Where Secret values live. Each Secret is one item, addressed by its
//! layer's owner and its name (`VariableLayer::secret_account`).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::LocalhostError;

pub trait SecretStore: Send + Sync {
    fn value(&self, account: &str) -> Result<Option<String>, LocalhostError>;
    fn set_value(&self, value: &str, account: &str) -> Result<(), LocalhostError>;
    fn remove_value(&self, account: &str) -> Result<(), LocalhostError>;
}

/// The service every Secret item is stored under, in the Keychain and the
/// Secret Service alike. Never change it: existing items would be lost.
pub const SERVICE: &str = "io.github.marcellosso.localhost.secret";
const LABEL: &str = "localhost secret";

#[cfg(target_os = "macos")]
pub type SystemSecretStore = keychain::KeychainSecretStore;
#[cfg(target_os = "linux")]
pub type SystemSecretStore = secret_service::SecretServiceStore;
#[cfg(windows)]
pub type SystemSecretStore = credential_manager::CredentialManagerStore;

/// How error messages name the system store.
#[cfg(target_os = "macos")]
pub(crate) const STORE_NAME: &str = "The Keychain";
#[cfg(windows)]
pub(crate) const STORE_NAME: &str = "Windows Credential Manager";
#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) const STORE_NAME: &str = "The secret store";

/// Secrets kept in memory: for tests, and anything that must never touch
/// the user's real store.
#[derive(Default)]
pub struct MemorySecretStore {
    items: Mutex<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `values` by account, removing those that are `None`, then
    /// forgets every account not in `kept`.
    pub fn update(&self, values: HashMap<String, Option<String>>, kept: &HashSet<String>) {
        let mut items = self.items.lock().unwrap();
        for (account, value) in values {
            match value {
                Some(value) => items.insert(account, value),
                None => items.remove(&account),
            };
        }
        items.retain(|account, _| kept.contains(account));
    }

    /// Every account with a stored value.
    pub fn accounts(&self) -> Vec<String> {
        let mut accounts: Vec<String> = self.items.lock().unwrap().keys().cloned().collect();
        accounts.sort();
        accounts
    }
}

impl SecretStore for MemorySecretStore {
    fn value(&self, account: &str) -> Result<Option<String>, LocalhostError> {
        Ok(self.items.lock().unwrap().get(account).cloned())
    }

    fn set_value(&self, value: &str, account: &str) -> Result<(), LocalhostError> {
        self.items.lock().unwrap().insert(account.to_string(), value.to_string());
        Ok(())
    }

    fn remove_value(&self, account: &str) -> Result<(), LocalhostError> {
        self.items.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod keychain {
    use security_framework::base::Error;
    use security_framework::passwords::{
        PasswordOptions, delete_generic_password, generic_password, set_generic_password_options,
    };

    use super::{LABEL, SERVICE, SecretStore};
    use crate::LocalhostError;

    /// `errSecItemNotFound`.
    const ITEM_NOT_FOUND: i32 = -25300;

    /// Stores each Secret as one generic-password item in the login Keychain.
    #[derive(Default)]
    pub struct KeychainSecretStore;

    impl KeychainSecretStore {
        pub fn new() -> Self {
            KeychainSecretStore
        }
    }

    fn failure(error: Error) -> LocalhostError {
        LocalhostError::SecretStoreFailed(error.message().unwrap_or_else(|| format!("error {}", error.code())))
    }

    impl SecretStore for KeychainSecretStore {
        fn value(&self, account: &str) -> Result<Option<String>, LocalhostError> {
            match generic_password(PasswordOptions::new_generic_password(SERVICE, account)) {
                Ok(data) => Ok(Some(String::from_utf8_lossy(&data).into_owned())),
                Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
                Err(error) => Err(failure(error)),
            }
        }

        fn set_value(&self, value: &str, account: &str) -> Result<(), LocalhostError> {
            let mut options = PasswordOptions::new_generic_password(SERVICE, account);
            options.set_label(LABEL);
            set_generic_password_options(value.as_bytes(), options).map_err(failure)
        }

        fn remove_value(&self, account: &str) -> Result<(), LocalhostError> {
            match delete_generic_password(SERVICE, account) {
                Err(error) if error.code() != ITEM_NOT_FOUND => Err(failure(error)),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod secret_service {
    use std::collections::HashMap;

    use secret_service::EncryptionType;
    use secret_service::blocking::{Collection, SecretService};

    use super::{LABEL, SERVICE, SecretStore};
    use crate::LocalhostError;

    /// Stores each Secret as one item in the default collection of the
    /// Secret Service (GNOME Keyring, KWallet), with the attributes
    /// `service` and `account`.
    #[derive(Default)]
    pub struct SecretServiceStore;

    impl SecretServiceStore {
        pub fn new() -> Self {
            SecretServiceStore
        }

        fn with_collection<T>(
            body: impl FnOnce(&Collection<'_>) -> Result<T, secret_service::Error>,
        ) -> Result<T, LocalhostError> {
            let service = SecretService::connect(EncryptionType::Dh).map_err(failure)?;
            let collection = service.get_default_collection().map_err(failure)?;
            collection.ensure_unlocked().map_err(failure)?;
            body(&collection).map_err(failure)
        }
    }

    fn failure(error: secret_service::Error) -> LocalhostError {
        LocalhostError::SecretStoreFailed(error.to_string())
    }

    fn attributes(account: &str) -> HashMap<&str, &str> {
        HashMap::from([("service", SERVICE), ("account", account)])
    }

    impl SecretStore for SecretServiceStore {
        fn value(&self, account: &str) -> Result<Option<String>, LocalhostError> {
            Self::with_collection(|collection| {
                let Some(item) = collection.search_items(attributes(account))?.into_iter().next() else {
                    return Ok(None);
                };
                Ok(Some(String::from_utf8_lossy(&item.get_secret()?).into_owned()))
            })
        }

        fn set_value(&self, value: &str, account: &str) -> Result<(), LocalhostError> {
            Self::with_collection(|collection| {
                collection.create_item(LABEL, attributes(account), value.as_bytes(), true, "text/plain")?;
                Ok(())
            })
        }

        fn remove_value(&self, account: &str) -> Result<(), LocalhostError> {
            Self::with_collection(|collection| {
                for item in collection.search_items(attributes(account))? {
                    item.delete()?;
                }
                Ok(())
            })
        }
    }
}

#[cfg(windows)]
mod credential_manager {
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
    };

    use super::{LABEL, SERVICE, SecretStore};
    use crate::LocalhostError;

    /// Stores each Secret as one generic credential in the user's Credential
    /// Manager, named `<service>/<account>`, with the account as its user name.
    #[derive(Default)]
    pub struct CredentialManagerStore;

    impl CredentialManagerStore {
        pub fn new() -> Self {
            CredentialManagerStore
        }
    }

    /// A NUL-terminated UTF-16 string.
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn target(account: &str) -> Vec<u16> {
        wide(&format!("{SERVICE}/{account}"))
    }

    fn last_error() -> u32 {
        unsafe { GetLastError() }
    }

    fn failure(code: u32) -> LocalhostError {
        LocalhostError::SecretStoreFailed(std::io::Error::from_raw_os_error(code as i32).to_string())
    }

    impl SecretStore for CredentialManagerStore {
        fn value(&self, account: &str) -> Result<Option<String>, LocalhostError> {
            let target = target(account);
            let mut credential: *mut CREDENTIALW = ptr::null_mut();
            if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
                return match last_error() {
                    ERROR_NOT_FOUND => Ok(None),
                    code => Err(failure(code)),
                };
            }
            let value = unsafe {
                let credential = &*credential;
                let blob =
                    std::slice::from_raw_parts(credential.CredentialBlob, credential.CredentialBlobSize as usize);
                String::from_utf8_lossy(blob).into_owned()
            };
            unsafe { CredFree(credential.cast()) };
            Ok(Some(value))
        }

        fn set_value(&self, value: &str, account: &str) -> Result<(), LocalhostError> {
            let mut target = target(account);
            let mut user_name = wide(account);
            let mut comment = wide(LABEL);
            let mut blob = value.as_bytes().to_vec();
            let credential = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: target.as_mut_ptr(),
                Comment: comment.as_mut_ptr(),
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: blob.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                UserName: user_name.as_mut_ptr(),
                ..unsafe { std::mem::zeroed() }
            };
            if unsafe { CredWriteW(&credential, 0) } == 0 {
                return Err(failure(last_error()));
            }
            Ok(())
        }

        fn remove_value(&self, account: &str) -> Result<(), LocalhostError> {
            let target = target(account);
            if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
                return match last_error() {
                    ERROR_NOT_FOUND => Ok(()),
                    code => Err(failure(code)),
                };
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the user's real store, so it only runs when asked:
    /// `cargo test -- --ignored system_store`.
    #[test]
    #[ignore]
    fn system_store_round_trips_a_value() {
        let store = SystemSecretStore::new();
        let account = format!("{}/LOCALHOST_TEST", crate::Id::new());
        assert_eq!(store.value(&account), Ok(None));
        store.set_value("one", &account).unwrap();
        store.set_value("two", &account).unwrap();
        assert_eq!(store.value(&account), Ok(Some("two".into())));
        store.remove_value(&account).unwrap();
        store.remove_value(&account).unwrap();
        assert_eq!(store.value(&account), Ok(None));
    }
}
