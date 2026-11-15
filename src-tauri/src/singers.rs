//! Voicebanks installed in OpenUtau, plus Muse Sing's own Singers folder. The singer id written in
//! a `.ustx` is the folder that holds `character.txt`, relative to the Singers folder (OpenUtau:
//! `Path.GetRelativePath`); voicebanks of Muse Sing's folder get the `musesing:` prefix, since
//! OpenUtau doesn't see them.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SingerInfo {
    pub id: String,
    pub name: String,
    pub singer_type: String,
    /// Languages with a DiffSinger dictionary (dsdict-<lang>.yaml).
    pub langs: Vec<String>,
    /// Absolute folder of the voicebank.
    pub path: String,
    /// Installed in OpenUtau (usable in a `.ustx`).
    pub openutau: bool,
    /// Muse Sing's engine can sing with it.
    pub engine: bool,
    /// Why it can't, for the UI.
    pub engine_note: Option<String>,
    /// Voice colours (subbanks).
    pub colors: Vec<String>,
}

pub const OWN_PREFIX: &str = "musesing:";

/// OpenUtau's data folder (PathManager.cs).
pub fn openutau_data_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    if cfg!(target_os = "macos") {
        Some(home.join("Library").join("OpenUtau"))
    } else {
        let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local").join("share"));
        Some(data.join("OpenUtau"))
    }
}

/// Singers folder plus the optional additional folder set in OpenUtau's preferences.
pub fn singer_roots() -> Vec<PathBuf> {
    let Some(data) = openutau_data_dir() else { return vec![] };
    let mut roots = vec![data.join("Singers")];
    if let Ok(text) = std::fs::read_to_string(data.join("prefs.json"))
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(p) = v.get("AdditionalSingerPath").and_then(|p| p.as_str()).filter(|p| !p.is_empty()) {
                roots.push(PathBuf::from(p));
            }
    roots
}

/// OpenUtau's singers, then those of `own` (Muse Sing's Singers folder).
pub fn list_singers(own: Option<&Path>) -> Vec<SingerInfo> {
    let mut out = vec![];
    for root in singer_roots() {
        walk(&root, &root, 0, "", &mut out);
    }
    if let Some(own) = own {
        walk(own, own, 0, OWN_PREFIX, &mut out);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, prefix: &str, out: &mut Vec<SingerInfo>) {
    if depth > 6 {
        return;
    }
    // OpenUtau's VoicebankLoader: every folder with a character.txt is a voicebank
    if dir.join("character.txt").is_file()
        && let Some(info) = read_singer(root, dir, prefix) {
            out.push(info);
        }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        walk(root, &d, depth + 1, prefix, out);
    }
}

/// (problem, colours) for Muse Sing's engine.
fn engine_check(dir: &Path, singer_type: &str) -> (Option<String>, Vec<String>) {
    if singer_type != "diffsinger" {
        return (Some("non è un voicebank DiffSinger".into()), vec![]);
    }
    match crate::ds::voicebank::Voicebank::load(dir) {
        Ok(vb) => (vb.engine_problem(), vb.colors()),
        Err(e) => (Some(e.to_string()), vec![]),
    }
}

fn read_singer(root: &Path, dir: &Path, prefix: &str) -> Option<SingerInfo> {
    let rel = dir.strip_prefix(root).ok()?;
    let id = prefix.to_string() + &rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
    let yaml: Option<serde_yaml::Value> =
        std::fs::read_to_string(dir.join("character.yaml")).ok().and_then(|t| serde_yaml::from_str(&t).ok());
    let get = |k: &str| yaml.as_ref().and_then(|y| y.get(k)).and_then(|v| v.as_str()).map(str::to_string);
    let name = get("name")
        .or_else(|| {
            std::fs::read_to_string(dir.join("character.txt"))
                .ok()?
                .lines()
                .find_map(|l| l.strip_prefix("name=").map(|n| n.trim().to_string()))
        })
        .unwrap_or_else(|| id.clone());
    let singer_type = get("singer_type").unwrap_or_else(|| "classic".into());
    let mut langs = vec![];
    find_dicts(dir, 0, &mut langs);
    langs.sort();
    langs.dedup();
    let (engine_note, colors) = engine_check(dir, &singer_type);
    Some(SingerInfo {
        id,
        name,
        singer_type,
        langs,
        path: dir.to_string_lossy().into_owned(),
        openutau: prefix.is_empty(),
        engine: engine_note.is_none(),
        engine_note,
        colors,
    })
}

fn find_dicts(dir: &Path, depth: usize, langs: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 2 {
            find_dicts(&p, depth + 1, langs);
        } else if let Some(name) = p.file_name().and_then(|n| n.to_str())
            && let Some(lang) = name.strip_prefix("dsdict-").and_then(|n| n.strip_suffix(".yaml"))
                && lang.len() == 2 {
                    langs.push(lang.to_string());
                }
    }
}

/// First-time choice for a language when the user hasn't picked one yet: for a `.ustx`
/// (OpenUtau's singers) or for Muse Sing's engine.
pub fn suggest(singers: &[SingerInfo], lang: &str, engine: bool) -> Option<String> {
    let preferred = match lang {
        "en" | "it" => "tiger",
        _ => "",
    };
    let ds = || singers.iter().filter(|s| s.singer_type == "diffsinger" && if engine { s.engine } else { s.openutau });
    ds().find(|s| !preferred.is_empty() && s.name.to_lowercase().contains(preferred))
        .or_else(|| ds().find(|s| s.langs.iter().any(|l| l == lang)))
        .or_else(|| ds().next())
        .map(|s| s.id.clone())
}
