//! Device keyring for the data-key passphrase.
//!
//! Linux: Secret Service (GNOME Keyring / KWallet).
//! Android: AES-256-GCM key in Android Keystore wrapping a blob in SharedPreferences.
//! Tests and `RURARENDAR_KEYRING_MEMORY` use an in-process map.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

use anyhow::Context;
use serde::{
    Deserialize,
    Serialize,
};

const SERVICE: &str = "net.onlyesterday.rurarendar";
const ACCOUNT: &str = "passphrase";

/// `old`: `None` means no rotation. `Some("")` means the previous data was plaintext.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passphrase {
    pub current: String,
    #[serde(default)]
    pub old: Option<String>,
}

impl Passphrase {
    pub fn is_empty(&self) -> bool {
        self.current.is_empty() && self.old.is_none()
    }
}

pub fn load() -> anyhow::Result<Passphrase> {
    let Some(json) = read_raw()? else {
        return Ok(Passphrase::default());
    };
    serde_json::from_str(&json).context("Invalid passphrase blob in the device keyring")
}

pub fn store(passphrase: &Passphrase) -> anyhow::Result<()> {
    if passphrase.is_empty() {
        return delete();
    }
    let json = serde_json::to_string(passphrase).context("Could not encode passphrase")?;
    write_raw(&json)
}

pub fn delete() -> anyhow::Result<()> {
    delete_raw()
}

pub fn clear_old() -> anyhow::Result<()> {
    let mut passphrase = load()?;
    if passphrase.old.is_none() {
        return Ok(());
    }
    passphrase.old = None;
    store(&passphrase)
}

fn use_memory() -> bool {
    cfg!(test) || std::env::var_os("RURARENDAR_KEYRING_MEMORY").is_some()
}

fn memory_slot() -> String {
    if cfg!(test) {
        format!("{:?}", std::thread::current().id())
    } else {
        "default".into()
    }
}

fn memory_map() -> &'static Mutex<HashMap<String, String>> {
    static MEMORY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    MEMORY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn memory_lock() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    memory_map().lock().unwrap_or_else(|err| err.into_inner())
}

fn read_raw() -> anyhow::Result<Option<String>> {
    if use_memory() {
        return Ok(memory_lock().get(&memory_slot()).cloned());
    }
    platform_read()
}

fn write_raw(json: &str) -> anyhow::Result<()> {
    if use_memory() {
        memory_lock().insert(memory_slot(), json.to_string());
        return Ok(());
    }
    platform_write(json)
}

fn delete_raw() -> anyhow::Result<()> {
    if use_memory() {
        memory_lock().remove(&memory_slot());
        return Ok(());
    }
    platform_delete()
}

#[cfg(target_os = "linux")]
fn platform_read() -> anyhow::Result<Option<String>> {
    let entry = linux_entry()?;
    match entry.get_password() {
        Ok(json) => Ok(Some(json)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(anyhow::anyhow!("Could not read the encryption passphrase from the system keyring: {err}")),
    }
}

#[cfg(target_os = "linux")]
fn platform_write(json: &str) -> anyhow::Result<()> {
    linux_entry()?
        .set_password(json)
        .map_err(|err| anyhow::anyhow!("Could not store the encryption passphrase in the system keyring: {err}"))
}

#[cfg(target_os = "linux")]
fn platform_delete() -> anyhow::Result<()> {
    match linux_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(anyhow::anyhow!("Could not delete the encryption passphrase from the system keyring: {err}")),
    }
}

#[cfg(target_os = "linux")]
fn linux_entry() -> anyhow::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|err| {
        anyhow::anyhow!("Could not open the system keyring (Secret Service, GNOME Keyring, or KWallet): {err}")
    })
}

#[cfg(target_os = "android")]
fn platform_read() -> anyhow::Result<Option<String>> {
    let json = android_call("load", None)?;
    if json.is_empty() {
        Ok(None)
    } else {
        Ok(Some(json))
    }
}

#[cfg(target_os = "android")]
fn platform_write(json: &str) -> anyhow::Result<()> {
    android_call("store", Some(json))?;
    Ok(())
}

#[cfg(target_os = "android")]
fn platform_delete() -> anyhow::Result<()> {
    android_call("delete", None)?;
    Ok(())
}

#[cfg(target_os = "android")]
fn android_call(method: &str, payload: Option<&str>) -> anyhow::Result<String> {
    use jni::{
        JavaVM,
        objects::{
            JObject,
            JString,
            JValue,
        },
    };

    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) };
    vm.attach_current_thread(|env| -> jni::errors::Result<String> {
        let context_object = unsafe { JObject::from_raw(env, context.context().cast()) };
        match method {
            "store" => {
                let json = JObject::from(env.new_string(payload.unwrap_or(""))?);
                env.call_static_method(
                    jni::jni_str!("net/onlyesterday/rurarendar/SecretBridge"),
                    jni::jni_str!("store"),
                    jni::jni_sig!("(Landroid/content/Context;Ljava/lang/String;)V"),
                    &[JValue::Object(&context_object), JValue::Object(&json)],
                )?;
                Ok(String::new())
            }
            "delete" => {
                env.call_static_method(
                    jni::jni_str!("net/onlyesterday/rurarendar/SecretBridge"),
                    jni::jni_str!("delete"),
                    jni::jni_sig!("(Landroid/content/Context;)V"),
                    &[JValue::Object(&context_object)],
                )?;
                Ok(String::new())
            }
            _ => {
                let result = env.call_static_method(
                    jni::jni_str!("net/onlyesterday/rurarendar/SecretBridge"),
                    jni::jni_str!("load"),
                    jni::jni_sig!("(Landroid/content/Context;)Ljava/lang/String;"),
                    &[JValue::Object(&context_object)],
                )?;
                let obj = result.l()?;
                if obj.is_null() {
                    return Ok(String::new());
                }
                let jstr = env.as_cast::<JString>(&obj)?;
                Ok(jstr.try_to_string(env)?)
            }
        }
    })
    .map_err(|err| anyhow::anyhow!("Android Keystore: {err}"))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn platform_read() -> anyhow::Result<Option<String>> {
    anyhow::bail!("the device keyring is not supported on this platform")
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn platform_write(_json: &str) -> anyhow::Result<()> {
    anyhow::bail!("the device keyring is not supported on this platform")
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn platform_delete() -> anyhow::Result<()> {
    anyhow::bail!("the device keyring is not supported on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated() {
        let _ = delete();
    }

    #[test]
    fn missing_entry_is_empty() {
        isolated();
        assert_eq!(load().unwrap(), Passphrase::default());
    }

    #[test]
    fn store_load_roundtrip_keeps_plaintext_origin() {
        isolated();
        let passphrase = Passphrase {
            current: "new-secret".into(),
            old: Some(String::new()),
        };
        store(&passphrase).unwrap();
        assert_eq!(load().unwrap(), passphrase);
        isolated();
    }

    #[test]
    fn empty_passphrase_deletes_the_entry() {
        isolated();
        store(&Passphrase {
            current: "secret".into(),
            old: None,
        })
        .unwrap();
        store(&Passphrase::default()).unwrap();
        assert_eq!(load().unwrap(), Passphrase::default());
    }

    #[test]
    fn clear_old_keeps_current() {
        isolated();
        store(&Passphrase {
            current: "new".into(),
            old: Some("old".into()),
        })
        .unwrap();
        clear_old().unwrap();
        assert_eq!(
            load().unwrap(),
            Passphrase {
                current: "new".into(),
                old: None,
            }
        );
        isolated();
    }
}
