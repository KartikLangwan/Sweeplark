use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Hive {
    #[serde(rename = "HKCU")]
    CurrentUser,
    #[serde(rename = "HKLM")]
    LocalMachine,
}

impl Hive {
    pub fn needs_admin_to_write(self) -> bool {
        self == Hive::LocalMachine
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RegValue {
    Dword(u32),
    Qword(u64),
    Sz(String),
    ExpandSz(String),
    MultiSz(Vec<String>),
    Binary(Vec<u8>),
}

impl RegValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            RegValue::Sz(s) | RegValue::ExpandSz(s) => Some(s),
            _ => None,
        }
    }
}

/// Minimal registry interface. The real one is Windows only; [`MemRegistry`] is used in tests.
pub trait RegistryBackend: Send + Sync {
    fn read_value(&self, hive: Hive, key: &str, name: &str) -> Result<Option<RegValue>>;
    fn write_value(&self, hive: Hive, key: &str, name: &str, value: &RegValue) -> Result<()>;
    fn delete_value(&self, hive: Hive, key: &str, name: &str) -> Result<()>;
    /// All values of a key. A missing key is an empty list, not an error.
    fn list_values(&self, hive: Hive, key: &str) -> Result<Vec<(String, RegValue)>>;
    /// Names of the direct subkeys of a key (empty if missing).
    fn list_subkeys(&self, _hive: Hive, _key: &str) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}

/// Split "HKLM\Software\X" into hive + key.
pub fn parse_key(full: &str) -> Option<(Hive, String)> {
    let (root, rest) = full.split_once('\\')?;
    let hive = match root.to_ascii_uppercase().as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => Hive::LocalMachine,
        "HKCU" | "HKEY_CURRENT_USER" => Hive::CurrentUser,
        _ => return None,
    };
    Some((hive, rest.to_string()))
}

/// In-memory registry for tests and non-Windows builds.
#[derive(Default)]
pub struct MemRegistry {
    data: Mutex<BTreeMap<(Hive, String), BTreeMap<String, RegValue>>>,
}

impl MemRegistry {
    fn k(key: &str) -> String {
        key.to_lowercase()
    }
}

impl RegistryBackend for MemRegistry {
    fn read_value(&self, hive: Hive, key: &str, name: &str) -> Result<Option<RegValue>> {
        let data = self.data.lock().unwrap();
        Ok(data.get(&(hive, Self::k(key))).and_then(|vals| vals.get(name)).cloned())
    }

    fn write_value(&self, hive: Hive, key: &str, name: &str, value: &RegValue) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.entry((hive, Self::k(key))).or_default().insert(name.to_string(), value.clone());
        Ok(())
    }

    fn delete_value(&self, hive: Hive, key: &str, name: &str) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        if let Some(vals) = data.get_mut(&(hive, Self::k(key))) {
            vals.remove(name);
        }
        Ok(())
    }

    fn list_values(&self, hive: Hive, key: &str) -> Result<Vec<(String, RegValue)>> {
        let data = self.data.lock().unwrap();
        Ok(data
            .get(&(hive, Self::k(key)))
            .map(|vals| vals.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default())
    }

    fn list_subkeys(&self, hive: Hive, key: &str) -> Result<Vec<String>> {
        let data = self.data.lock().unwrap();
        let prefix = format!("{}\\", Self::k(key));
        let mut out: Vec<String> = data
            .keys()
            .filter(|(h, k)| *h == hive && k.starts_with(&prefix))
            .filter_map(|(_, k)| k[prefix.len()..].split('\\').next().map(String::from))
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
    }
}

/// The registry of this machine. On non-Windows systems this is an empty in-memory registry.
pub fn system_registry() -> Box<dyn RegistryBackend> {
    #[cfg(windows)]
    {
        Box::new(win::WinRegistry)
    }
    #[cfg(not(windows))]
    {
        Box::new(MemRegistry::default())
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use crate::Error;
    use winreg::enums::*;
    use winreg::{RegKey, RegValue as WinValue};

    pub struct WinRegistry;

    fn root(hive: Hive) -> RegKey {
        match hive {
            Hive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
            Hive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
        }
    }

    fn map_err(e: std::io::Error, what: &str) -> Error {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::NeedsAdmin(what.to_string())
        } else {
            Error::Registry(format!("{what}: {e}"))
        }
    }

    fn utf16_bytes_to_string(bytes: &[u8]) -> String {
        let wide: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        let s = String::from_utf16_lossy(&wide);
        s.trim_end_matches('\0').to_string()
    }

    fn string_to_utf16_bytes(s: &str) -> Vec<u8> {
        s.encode_utf16().chain(std::iter::once(0)).flat_map(|c| c.to_le_bytes()).collect()
    }

    fn from_win(v: &WinValue) -> Option<RegValue> {
        Some(match v.vtype {
            REG_DWORD if v.bytes.len() >= 4 => {
                RegValue::Dword(u32::from_le_bytes(v.bytes[..4].try_into().ok()?))
            }
            REG_QWORD if v.bytes.len() >= 8 => {
                RegValue::Qword(u64::from_le_bytes(v.bytes[..8].try_into().ok()?))
            }
            REG_SZ => RegValue::Sz(utf16_bytes_to_string(&v.bytes)),
            REG_EXPAND_SZ => RegValue::ExpandSz(utf16_bytes_to_string(&v.bytes)),
            REG_MULTI_SZ => RegValue::MultiSz(
                utf16_bytes_to_string(&v.bytes).split('\0').filter(|s| !s.is_empty()).map(String::from).collect(),
            ),
            REG_BINARY => RegValue::Binary(v.bytes.to_vec()),
            _ => return None,
        })
    }

    fn to_win(v: &RegValue) -> WinValue<'static> {
        let (vtype, bytes) = match v {
            RegValue::Dword(d) => (REG_DWORD, d.to_le_bytes().to_vec()),
            RegValue::Qword(q) => (REG_QWORD, q.to_le_bytes().to_vec()),
            RegValue::Sz(s) => (REG_SZ, string_to_utf16_bytes(s)),
            RegValue::ExpandSz(s) => (REG_EXPAND_SZ, string_to_utf16_bytes(s)),
            RegValue::MultiSz(list) => {
                let mut joined = list.join("\0");
                joined.push('\0');
                (REG_MULTI_SZ, string_to_utf16_bytes(&joined))
            }
            RegValue::Binary(b) => (REG_BINARY, b.clone()),
        };
        WinValue { vtype, bytes: bytes.into() }
    }

    impl RegistryBackend for WinRegistry {
        fn read_value(&self, hive: Hive, key: &str, name: &str) -> Result<Option<RegValue>> {
            let k = match root(hive).open_subkey_with_flags(key, KEY_READ) {
                Ok(k) => k,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(map_err(e, key)),
            };
            match k.get_raw_value(name) {
                Ok(v) => Ok(from_win(&v)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(map_err(e, key)),
            }
        }

        fn write_value(&self, hive: Hive, key: &str, name: &str, value: &RegValue) -> Result<()> {
            let (k, _) = root(hive).create_subkey_with_flags(key, KEY_WRITE).map_err(|e| map_err(e, key))?;
            k.set_raw_value(name, &to_win(value)).map_err(|e| map_err(e, key))
        }

        fn delete_value(&self, hive: Hive, key: &str, name: &str) -> Result<()> {
            let k = match root(hive).open_subkey_with_flags(key, KEY_WRITE) {
                Ok(k) => k,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(map_err(e, key)),
            };
            match k.delete_value(name) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(map_err(e, key)),
            }
        }

        fn list_values(&self, hive: Hive, key: &str) -> Result<Vec<(String, RegValue)>> {
            let k = match root(hive).open_subkey_with_flags(key, KEY_READ) {
                Ok(k) => k,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(e) => return Err(map_err(e, key)),
            };
            let mut out = Vec::new();
            for item in k.enum_values() {
                let (name, value) = item.map_err(|e| map_err(e, key))?;
                if let Some(v) = from_win(&value) {
                    out.push((name, v));
                }
            }
            Ok(out)
        }

        fn list_subkeys(&self, hive: Hive, key: &str) -> Result<Vec<String>> {
            match root(hive).open_subkey_with_flags(key, KEY_READ) {
                Ok(k) => Ok(k.enum_keys().flatten().collect()),
                Err(_) => Ok(Vec::new()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_registry_roundtrip() {
        let r = MemRegistry::default();
        r.write_value(Hive::CurrentUser, r"Software\X", "a", &RegValue::Dword(1)).unwrap();
        assert_eq!(r.read_value(Hive::CurrentUser, r"software\x", "a").unwrap(), Some(RegValue::Dword(1)));
        assert_eq!(r.list_values(Hive::CurrentUser, r"Software\X").unwrap().len(), 1);
        r.delete_value(Hive::CurrentUser, r"Software\X", "a").unwrap();
        assert_eq!(r.read_value(Hive::CurrentUser, r"Software\X", "a").unwrap(), None);
        assert!(r.list_values(Hive::LocalMachine, "missing").unwrap().is_empty());
    }

    #[test]
    fn reg_value_json_shape() {
        let json = serde_json::to_string(&RegValue::Binary(vec![2, 0])).unwrap();
        assert_eq!(json, r#"{"kind":"binary","data":[2,0]}"#);
    }
}
