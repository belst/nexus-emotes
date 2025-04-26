use crate::seventv::EmoteSet;
use crate::util::{UiExt, e};
use anyhow::Result;
use nexus::imgui::Ui;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard, OnceLock};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Diff<T: Debug + Clone + Hash + PartialEq + Eq> {
    Added(T),
    Removed(T),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub emote_set_ids: Vec<String>,
    pub use_global: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            emote_set_ids: Vec::new(),
            use_global: true,
        }
    }
}

static SETTINGS: OnceLock<Mutex<Settings>> = OnceLock::new();

impl Settings {
    pub fn get() -> MutexGuard<'static, Self> {
        let mtx = SETTINGS.get_or_init(|| Mutex::new(Settings::default()));
        mtx.lock().unwrap()
    }

    pub fn load(&mut self, path: &impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(());
        }
        let settings = std::fs::read_to_string(path)?;
        *self = serde_json::from_str(&settings)?;
        Ok(())
    }

    pub fn save(&self, path: &impl AsRef<std::path::Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn ui_and_save(
        &mut self,
        emote_sets: &[EmoteSet],
        ui: &Ui,
    ) -> Option<HashSet<Diff<String>>> {
        thread_local! {
            static DIFF: RefCell<HashSet<Diff<String>>> = RefCell::new(HashSet::new());
        }
        let old_use_global = self.use_global;
        ui.checkbox(e("Use global 7tv Emote Set"), &mut self.use_global);
        if ui.help_marker(|| {
            ui.tooltip_text(e("Enable 7tv global emote set. Click to open in browser"));
        }) {
            if let Err(e) =
                open::that_detached("https://7tv.app/emote-sets/01HKQT8EWR000ESSWF3625XCS4")
            {
                log::error!("Failed to open browser: {e}");
            }
        }
        if old_use_global != self.use_global {
            DIFF.with_borrow_mut(|d| {
                if self.use_global {
                    d.remove(&Diff::Removed("global".to_string()));
                    d.insert(Diff::Added("global".to_string()));
                } else {
                    d.remove(&Diff::Added("global".to_string()));
                    d.insert(Diff::Removed("global".to_string()));
                }
            });
        }
        let t = ui.begin_table("emote sets", 2);
        let mut to_remove = Vec::new();
        for (i, id) in self.emote_set_ids.iter().enumerate() {
            ui.table_next_row();
            ui.table_next_column();
            if let Some(es) = emote_sets.iter().find(|es| &es.id == id) {
                ui.link(&es.name, format!("https://7tv.app/emote-sets/{id}"));
            } else {
                ui.link(id, format!("https://7tv.app/emote-sets/{id}"));
            }
            ui.table_next_column();
            if ui.button(e("Remove") + &format!("##emotesetremove{i}")) {
                to_remove.push(i);
                DIFF.with_borrow_mut(|d| {
                    d.remove(&Diff::Added(id.clone()));
                    d.insert(Diff::Removed(id.clone()))
                });
            }
        }
        for tr in to_remove {
            self.emote_set_ids.remove(tr);
        }
        ui.table_next_row();
        ui.table_next_column();
        thread_local! {
            static ID: RefCell<String> = const { RefCell::new(String::new()) };
        }
        ID.with_borrow_mut(|mut id| {
            ui.input_text(e("ID") + "##emotesetinput", &mut id).build();
            ui.help_marker(|| {
                ui.tooltip_text(e(
                    "User ID or Emote Set ID (on 7tv in the url after /emote-sets/)",
                ));
            });
            ui.table_next_column();
            if ui.button(e("Add") + "##dpsreportfilterid") {
                self.emote_set_ids.push(id.clone());
                DIFF.with_borrow_mut(|d| {
                    d.remove(&Diff::Removed(id.clone()));
                    d.insert(Diff::Added(id.clone()));
                });
                id.clear();
            }
        });
        drop(t);
        if ui.button(e("Save")) {
            Some(DIFF.take())
        } else {
            None
        }
    }
}
