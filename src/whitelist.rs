use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WhitelistEntry {
    pub uuid: String,
    pub name: String,
}

pub fn get_offline_uuid(username: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{}", username).as_bytes());
    let mut hash = *digest;
    
    // Встановлюємо версію 3 (MD5) та варіант IETF (RFC 4122)
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3],
        hash[4], hash[5],
        hash[6], hash[7],
        hash[8], hash[9],
        hash[10], hash[11], hash[12], hash[13], hash[14], hash[15]
    )
}

pub fn load_whitelist(server_path: &Path) -> Vec<WhitelistEntry> {
    let path = server_path.join("whitelist.json");
    if path.exists() {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(entries) = serde_json::from_str::<Vec<WhitelistEntry>>(&content) {
                return entries;
            }
        }
    }
    Vec::new()
}

pub fn save_whitelist(server_path: &Path, entries: &[WhitelistEntry]) -> Result<(), std::io::Error> {
    let path = server_path.join("whitelist.json");
    let content = serde_json::to_string_pretty(entries)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn add_to_whitelist(server_path: &Path, username: &str) -> Result<(), std::io::Error> {
    let username_trimmed = username.trim();
    if username_trimmed.is_empty() {
        return Ok(());
    }
    
    let mut entries = load_whitelist(server_path);
    // Перевіряємо чи вже є користувач у списку (без врахування регістру)
    if !entries.iter().any(|e| e.name.eq_ignore_ascii_case(username_trimmed)) {
        let uuid = get_offline_uuid(username_trimmed);
        entries.push(WhitelistEntry {
            uuid,
            name: username_trimmed.to_string(),
        });
        save_whitelist(server_path, &entries)?;
    }
    Ok(())
}

pub fn remove_from_whitelist(server_path: &Path, username: &str) -> Result<(), std::io::Error> {
    let mut entries = load_whitelist(server_path);
    let initial_len = entries.len();
    entries.retain(|e| !e.name.eq_ignore_ascii_case(username.trim()));
    if entries.len() != initial_len {
        save_whitelist(server_path, &entries)?;
    }
    Ok(())
}
