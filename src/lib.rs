#![feature(lock_value_accessors)]

use background::{RunningWorker, Worker};
use giftex::{Gif, GifState};
use nexus::arcdps::extras::message::{ChatMessageInfo, RawChatMessageInfo};
use nexus::data_link::read_nexus_link;
use nexus::gui::{RenderType, register_render, render};
use nexus::imgui::{Condition, Image, Ui, Window};
use nexus::paths::get_addon_dir;
use nexus::texture::{Texture, get_texture, get_texture_or_create_from_url};
use nexus::{AddonApi, event_consume};
use nexus::{AddonFlags, UpdateProvider, event::extras::CHAT_MESSAGE as UE_CHAT_MESSAGE};
use settings::{Diff, Settings};
use seventv::{EmoteSet, download_emote_sets, get_emotes};
use std::cell::Cell;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use crate::chat_events::{CHAT_MESSAGE as CE_CHAT_MESSAGE, Message, raw::Message as RawMessage};
use crate::settings::ChatMessageSource;

mod background;
mod chat_events;
mod chat_message;
mod giftex;
mod settings;
mod seventv;
mod util;

fn setting_path() -> PathBuf {
    get_addon_dir(env!("CARGO_PKG_NAME"))
        .expect("Addon dir to exist")
        .join("settings.json")
}

#[derive(Debug, Clone)]
struct EmoteLayer {
    identifier: String,
    gif: Option<GifState>,
}

#[derive(Debug, Clone)]
struct ActiveEmote {
    layers: (EmoteLayer, Option<EmoteLayer>),
    position: Option<[f32; 2]>,
    start: Option<Instant>,
    start_offset: f32,
}

const SPEED: f32 = 0.5;
impl ActiveEmote {
    fn simulate(&mut self, elapsed: f32) {
        let speed = SPEED
            + (self.start_offset + self.start.unwrap().elapsed().as_millis() as f32 / 1000.0).sin()
                * 0.1;
        if let Some(position) = self.position {
            let [x, y] = position;
            self.position = Some([x, y - speed * elapsed]);
        }
    }
    fn get_position(&self, padding_width: f32) -> [f32; 2] {
        let position = self.position.unwrap_or([0.0, 0.0]);
        [
            position[0]
                + (self.start_offset + self.start.unwrap().elapsed().as_millis() as f32 / 1000.0)
                    .sin()
                    * padding_width,
            position[1],
        ]
    }
}

static ACTIVE_EMOTES: Mutex<Vec<ActiveEmote>> = const { Mutex::new(Vec::new()) };
static EMOTE_SETS: Mutex<Vec<EmoteSet>> = const { Mutex::new(Vec::new()) };
static WORKER: OnceLock<Mutex<Option<RunningWorker>>> = const { OnceLock::new() };
static LOADED_EMOTES: Mutex<Vec<(String, Option<Gif>)>> = const { Mutex::new(Vec::new()) };

fn load() {
    log::info!("Loading Meme Message");
    let mut settings = Settings::get();
    if let Err(e) = settings.load(&setting_path()) {
        log::error!("Failed to load settings: {e}");
    }
    let lock = WORKER
        .get_or_init(|| Mutex::new(Some(Worker::new().run())))
        .lock()
        .unwrap();
    let worker = lock.as_ref().expect("Option to be set");
    let settings = settings.clone();
    worker.spawn(Box::new(move || {
        let emote_sets = download_emote_sets(&settings.emote_set_ids, settings.use_global);
        *EMOTE_SETS.lock().unwrap() = emote_sets;
    }));
    register_render(RenderType::Render, render!(render_fn)).revert_on_unload();
    register_render(RenderType::OptionsRender, render!(render_options)).revert_on_unload();
    // TODO: this event is not triggered, if you are already in a squad when logging in
    UE_CHAT_MESSAGE
        .subscribe(event_consume!(|payload: Option<&RawChatMessageInfo>| {
            if let Some(payload) = payload {
                chat_message_ue(payload.into());
            }
        }))
        .revert_on_unload();
    // extern "C-unwind" fn event_callback(ptr: *const c_void) {
    //     let payload = ptr as *const *const i8;
    //     log::trace!("Decoded message: {:#04x?}", unsafe {
    //         std::slice::from_raw_parts(*payload, 10)
    //     });
    // }
    // unsafe { (AddonApi::get().event.subscribe)(c"EV_CHAT:Message".as_ptr(), event_callback) }
    CE_CHAT_MESSAGE
        .subscribe(event_consume!(|payload: Option<&RawMessage>| {
            if let Some(&payload) = payload {
                chat_message_ce(payload);
            }
        }))
        .revert_on_unload();
}

fn render_options(ui: &Ui) {
    let mut settings = Settings::get();
    let mut emote_sets = EMOTE_SETS.lock().unwrap();
    // Check for source status here
    if let Some(diff) = settings.ui_and_save(emote_sets.as_slice(), ui) {
        settings.save(&setting_path()).unwrap();
        for d in diff {
            match d {
                Diff::Added(id) => {
                    // Do we care about the case where we change settings during download?
                    let lock = WORKER.wait().lock().unwrap();
                    let worker = lock.as_ref().expect("Option to be set");
                    worker.spawn(Box::new(move || {
                        let Ok(emote_set) = get_emotes(&id) else {
                            log::error!("Failed to download emote set: {id}");
                            return;
                        };
                        let mut emote_sets = EMOTE_SETS.lock().unwrap();
                        emote_sets.push(emote_set);
                    }));
                }
                Diff::Removed(id) => {
                    emote_sets.retain(|e| e.id != id);
                }
            }
        }
    }
}

fn random_offset(range: RangeInclusive<f32>) -> f32 {
    rand::random_range(range)
}

enum EmoteType {
    Static(Texture),
    Gif(GifState),
}

impl EmoteType {
    fn from_texture(texture: Texture) -> Self {
        Self::Static(texture)
    }

    fn from_gif(gif: GifState) -> Self {
        Self::Gif(gif)
    }

    fn width(&self) -> f32 {
        match self {
            EmoteType::Static(t) => t.width as f32,
            EmoteType::Gif(g) => g.frames.width,
        }
    }

    fn height(&self) -> f32 {
        match self {
            EmoteType::Static(t) => t.height as f32,
            EmoteType::Gif(g) => g.frames.height,
        }
    }
}

fn check_gif(active_emote: &mut EmoteLayer) {
    if let Some(gif) = LOADED_EMOTES.lock().unwrap().iter_mut().find_map(|(l, r)| {
        if l == &active_emote.identifier {
            r.as_ref()
        } else {
            None
        }
    }) {
        active_emote.gif = Some(GifState::new(gif.clone()));
    }
}

fn update_gifs(device: &ID3D11Device) {
    let mut loaded = LOADED_EMOTES.lock().unwrap();
    let gifs = match giftex::process_queue(device) {
        Ok(gifs) => gifs,
        Err(e) => {
            log::error!("Failed to process gif queue: {e}");
            return;
        }
    };
    for (identifier, gif) in gifs {
        if let Some(e) = loaded.iter_mut().find(|(l, _)| l == &identifier) {
            e.1 = Some(gif);
        }
    }
}

fn get_textures(active_emote: &mut ActiveEmote) -> Option<(EmoteType, Option<EmoteType>)> {
    let texture_base = get_texture(&active_emote.layers.0.identifier);
    if active_emote.layers.0.gif.is_none() && texture_base.is_none() {
        check_gif(&mut active_emote.layers.0);
        return None;
    }
    let texture_base = texture_base
        .map(EmoteType::from_texture)
        .or_else(|| active_emote.layers.0.gif.take().map(EmoteType::from_gif))
        .expect("Texture or gif should exist here");

    if let Some(overlay) = active_emote.layers.1.as_mut() {
        let texture_overlay = get_texture(&overlay.identifier);
        if overlay.gif.is_none() && texture_overlay.is_none() {
            check_gif(overlay);
            return None;
        }
        let texture_overlay = texture_overlay
            .map(EmoteType::from_texture)
            .or_else(|| overlay.gif.take().map(EmoteType::from_gif))
            .expect("Texture or gif should exist here");
        Some((texture_base, Some(texture_overlay)))
    } else {
        Some((texture_base, None))
    }
}

fn render_fn(ui: &Ui) {
    let device = AddonApi::get().get_d3d11_device().expect("Device to exist");
    update_gifs(&device);
    thread_local! {
        static LAST_TS: Cell<Instant> = Cell::new(Instant::now());
    }
    let elapsed = LAST_TS.get().elapsed().as_millis() as f32;
    const PADDING: f32 = 0.10;
    let mut active_emotes = ACTIVE_EMOTES.lock().unwrap();
    let ndata = read_nexus_link().expect("Nexuslink to exist");
    let mut to_remove = Vec::new();
    for (i, active_emote) in active_emotes.iter_mut().enumerate() {
        let Some((base, overlay)) = get_textures(active_emote) else {
            continue;
        };
        let width = overlay
            .as_ref()
            .map_or(base.width(), |o| o.width().max(base.width()));
        let height = overlay
            .as_ref()
            .map_or(base.height(), |o| o.height().max(base.height()));
        if active_emote.position.is_none() {
            let factual_width = ndata.width as f32 - width / 2.0;
            let left_offset = factual_width * PADDING;
            let right_offset = factual_width * (1.0 - PADDING);
            active_emote.position = Some([
                random_offset(left_offset..=right_offset) - width / 2.0,
                ndata.height as f32,
            ]);
        }
        if active_emote.start.is_none() {
            active_emote.start = Some(Instant::now());
        }
        active_emote.simulate(elapsed);
        let pos = active_emote.get_position(ndata.width as f32 * PADDING / 2.0);
        if (pos[1] + height) < 0.0 {
            to_remove.push(i);
        } else if let Some(_w) = Window::new(format!("EMOTE#{i}"))
            .no_decoration()
            .always_auto_resize(true)
            .draw_background(false)
            .movable(false)
            .no_inputs()
            .focus_on_appearing(false)
            .position(pos, Condition::Always)
            .begin(ui)
        {
            ui.set_cursor_pos([(width - base.width()) / 2.0, (height - base.height()) / 2.0]);
            match base {
                EmoteType::Static(texture) => {
                    Image::new(texture.id(), texture.size()).build(ui);
                }
                EmoteType::Gif(mut gif) => {
                    gif.advance(ui);
                    active_emote.layers.0.gif = Some(gif);
                }
            }
            if let Some(overlay) = overlay {
                ui.set_cursor_pos([
                    (width - overlay.width()) / 2.0,
                    (height - overlay.height()) / 2.0,
                ]);
                match overlay {
                    EmoteType::Static(texture) => {
                        Image::new(texture.id(), texture.size()).build(ui);
                    }
                    EmoteType::Gif(mut gif) => {
                        gif.advance(ui);
                        active_emote.layers.1.as_mut().unwrap().gif = Some(gif);
                    }
                }
            }
        }
    }
    for i in to_remove.into_iter().rev() {
        log::info!("Removing emote #{i}");
        drop(active_emotes.swap_remove(i));
    }
    LAST_TS.set(Instant::now());
}

fn unload() {
    WORKER
        .wait()
        .replace(None)
        .unwrap()
        .expect("Option to be set")
        .join();
    drop(ACTIVE_EMOTES.replace(Vec::new()));
    drop(EMOTE_SETS.replace(Vec::new()));
}

fn chat_message_ue(message: ChatMessageInfo<'_>) {
    if !matches!(
        Settings::get().chat_message_source,
        ChatMessageSource::UnofficialExtras
    ) {
        return;
    }
    let message = message.to_owned();
    process_message(message.into());
}
fn chat_message_ce(message: RawMessage) {
    if !matches!(
        Settings::get().chat_message_source,
        ChatMessageSource::ChatEvents
    ) {
        return;
    }
    let message: Message = match message.try_into() {
        Ok(message) => message,
        Err(e) => {
            log::error!("Failed to convert raw message to message: {e}");
            return;
        }
    };
    process_message(message);
}

// TODO: filter based on source/settings
fn process_message(chat: Message) {
    let Some(content) = chat.content() else {
        return;
    };
    let mut loaded = LOADED_EMOTES.lock().unwrap();
    let emote_sets = EMOTE_SETS.lock().unwrap();
    let mut last_was_emote = false;
    let mut active_emotes = ACTIVE_EMOTES.lock().unwrap();
    for word in content.split_whitespace() {
        let mut is_emote = false;
        // TODO: if an emote is in multiple sets, only the last one can have a zero
        // width emote
        for emote in emote_sets.iter().flat_map(|e| e.emotes.iter()) {
            if emote.name == word {
                log::info!("Found emote {word} in chat message");
                let identifier = format!("EMOTE_{word}");
                if last_was_emote && emote.zero_width() {
                    log::info!("Found zero width emote {word}");
                    let last = active_emotes
                        .last_mut()
                        .expect("Last Active Emote to Exist");
                    last.layers.1 = Some(EmoteLayer {
                        identifier: identifier.clone(),
                        gif: None,
                    });
                } else {
                    is_emote = true;
                    active_emotes.push(ActiveEmote {
                        layers: (
                            EmoteLayer {
                                identifier: identifier.clone(),
                                gif: None,
                            },
                            None,
                        ),
                        position: None,
                        start: None,
                        start_offset: rand::random(),
                    });
                }
                if loaded.iter().any(|(l, _)| l == &identifier) {
                    continue;
                }
                log::info!("Loading emote {word}");
                if let Some(file) = emote.find_file() {
                    let Ok(url) = url::Url::parse(&format!("https:{}/", emote.data.host.url))
                    else {
                        log::error!("Failed to parse url: {}", emote.data.host.url);
                        continue;
                    };
                    let Ok(url) = url.join(&file.name) else {
                        log::error!("Failed to join url: {}", file.name);
                        continue;
                    };
                    // just trigger load
                    // there should be a load_texture_from_url function
                    // but apparently the bindings don't expose it yet
                    loaded.push((identifier.clone(), None));
                    if emote.data.animated {
                        let lock = WORKER.wait().lock().unwrap();
                        let worker = lock.as_ref().expect("Option to be set");
                        worker.spawn(Box::new(move || {
                            if let Err(e) = Gif::load(identifier.clone(), url.as_str()) {
                                log::error!("Failed to load gif: {e}");
                            };
                        }));
                    } else {
                        let _ = get_texture_or_create_from_url(
                            &identifier,
                            url.origin().ascii_serialization(),
                            url.path(),
                        );
                    }
                }
            }
        }
        last_was_emote = is_emote;
    }
}

nexus::export! {
    name: "Emote Chat",
    signature: -69423,
    flags: AddonFlags::None,
    load,
    unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/belst/nexus-emotes",
    log_filter: "warn,nexus_emotes=trace"
}
