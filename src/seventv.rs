use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::iter;

// Represents an owner with dynamic style.
#[derive(Debug, Serialize, Deserialize)]
pub struct Owner {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub style: Value,
    pub roles: Vec<String>,
}

// Enum for File.format with variants for "AVIF" and "WEBP".
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum FileFormat {
    Avif,
    Webp,
    Gif,
    Png,
    #[serde(other)]
    Unknown,
}

// Represents a file.
#[derive(Debug, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    pub static_name: String,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    pub size: u32,
    pub format: FileFormat,
}

// Represents the host containing URL and a list of files.
#[derive(Debug, Serialize, Deserialize)]
pub struct Host {
    pub url: String,
    pub files: Vec<File>,
}

// Extra data for an emote.
#[derive(Debug, Serialize, Deserialize)]
pub struct EmoteData {
    pub id: String,
    pub name: String,
    pub state: Vec<String>,
    pub listed: bool,
    pub animated: bool,
    pub owner: Owner,
    pub host: Host,
}

// Represents an emote.
#[derive(Debug, Serialize, Deserialize)]
pub struct Emote {
    pub id: String,
    pub name: String,
    pub flags: u32,
    pub timestamp: u64,
    pub actor_id: Value, // using Value for unknown actor_id
    pub data: EmoteData,
}

// Represents an emote set.
#[derive(Debug, Serialize, Deserialize)]
pub struct EmoteSet {
    pub id: String,
    pub name: String,
    pub flags: u32,
    pub tags: Vec<Value>, // unknown type becomes Value
    pub immutable: bool,
    pub privileged: bool,
    pub emotes: Vec<Emote>,
    pub emote_count: u32,
    pub capacity: u32,
    pub owner: Owner,
}

pub fn get_emotes(emote_id: &str) -> Result<EmoteSet> {
    let url = format!("https://7tv.io/v3/emote-sets/{}", emote_id);

    let emote_set = ureq::get(&url).call()?.body_mut().read_json()?;
    // who even needs more error handling setps

    Ok(emote_set)
}

pub fn download_emote_sets(emote_set_ids: &[String], use_global: bool) -> Vec<EmoteSet> {
    let mut it: Box<dyn Iterator<Item = _>> = Box::new(emote_set_ids.iter().map(String::as_str));
    if use_global {
        it = Box::new(it.chain(iter::once("global")));
    }
    let (ok, err): (Vec<_>, Vec<_>) = it.map(get_emotes).partition(Result::is_ok);
    for e in err {
        // noop
        if let Err(e) = e {
            log::error!("Failed to download emote set: {}", e);
        }
    }
    ok.into_iter().map(Result::unwrap).collect()
}
