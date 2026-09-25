use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, value};

use crate::config;
use crate::notices;

// Writes go through toml_edit so the comments and layout of a hand-edited file survive.
pub fn run(args: &[String]) -> Option<Result<String, String>> {
    let path = config::dir().join("config.toml");
    match args {
        [command, key] if command == "get" => Some(get(&path, key)),
        [command, key, raw] if command == "set" => Some(set(&path, key, raw).map(|()| String::new())),
        [] => None,
        _ => Some(Err(notices::usage())),
    }
}

fn effective(path: &Path) -> Result<toml::Value, String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let config = config::parse(&text).map_err(|reason| notices::bad_config(path, &reason))?;
    toml::Value::try_from(&config).map_err(|e| e.to_string())
}

fn lookup<'a>(root: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    key.split('.').try_fold(root, |node, part| node.get(part))
}

fn get(path: &Path, key: &str) -> Result<String, String> {
    let root = effective(path)?;
    match lookup(&root, key).ok_or_else(|| notices::unknown_key(key))? {
        toml::Value::String(text) => Ok(text.clone()),
        // Settings are f32; printing through f32 gives 0.3, not 0.30000001192092896.
        toml::Value::Float(number) => Ok((*number as f32).to_string()),
        other => Ok(other.to_string()),
    }
}

fn typed(current: &toml::Value, key: &str, raw: &str) -> Result<Item, String> {
    let bad = || notices::bad_value(key, raw);
    Ok(match current {
        toml::Value::Float(_) => value(raw.parse::<f64>().map_err(|_| bad())?),
        toml::Value::Integer(_) => value(raw.parse::<i64>().map_err(|_| bad())?),
        toml::Value::Boolean(_) => value(raw.parse::<bool>().map_err(|_| bad())?),
        toml::Value::String(_) => value(raw),
        _ => return Err(bad()),
    })
}

fn set(path: &Path, key: &str, raw: &str) -> Result<(), String> {
    let root = effective(path)?;
    let (section, name) = key.split_once('.').filter(|(_, name)| !name.contains('.')).ok_or_else(|| notices::unknown_key(key))?;
    let current = lookup(&root, key).ok_or_else(|| notices::unknown_key(key))?;
    let item = typed(current, key, raw)?;
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut document: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| notices::bad_config(path, e.message()))?;
    let table = document.entry(section).or_insert(Item::Table(Table::new()));
    let table = table.as_table_mut().ok_or_else(|| notices::unknown_key(key))?;
    table.insert(name, item);
    let updated = document.to_string();
    config::parse(&updated).map_err(|reason| notices::bad_value_reason(key, raw, &reason))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension("toml.new");
    std::fs::write(&temporary, updated).map_err(|e| e.to_string())?;
    std::fs::rename(&temporary, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mnv-cli-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn get_reads_defaults_and_file_values() {
        let path = scratch("get", "[palette]\npreset = \"amber\"\n");
        assert_eq!(get(&path, "palette.preset").unwrap(), "amber");
        assert_eq!(get(&path, "rain.fall_speed").unwrap(), "0.2");
        assert!(get(&path, "rain.nope").is_err());
    }

    #[test]
    fn set_keeps_comments_and_checks_types() {
        let path = scratch("set", "# mine\n[rain]\nfall_speed = 0.3 # slow\n");
        assert!(set(&path, "rain.fall_speed", "fast").is_err());
        assert!(set(&path, "rain.nope", "1").is_err());
        set(&path, "rain.fall_speed", "0.5").unwrap();
        set(&path, "palette.preset", "ice").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# mine"));
        assert_eq!(get(&path, "rain.fall_speed").unwrap(), "0.5");
        assert_eq!(get(&path, "palette.preset").unwrap(), "ice");
    }
}
