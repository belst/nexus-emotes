use nexus::arcdps::extras::message::{ChatMessageInfo, ChatMessageInfoOwned, RawChatMessageInfo};
use nexus::event_consume;
use nexus::gui::{RenderType, register_render, render};
use nexus::imgui::{Image, StyleVar, Ui, Window};
use nexus::paths::get_addon_dir;
use nexus::texture::{Texture, get_texture, get_texture_or_create_from_file};
use nexus::{AddonFlags, UpdateProvider, event::extras::CHAT_MESSAGE};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn emote_path() -> PathBuf {
    get_addon_dir(env!("CARGO_PKG_NAME"))
        .expect("Addon dir to exist")
        .join("emotes")
}

#[derive(Debug, Clone)]
enum Span {
    Emote(EmoteSpan),
    Text(String),
}

#[derive(Debug, Clone)]
struct ProcessedMessage {
    chat: ChatMessageInfoOwned,
    spans: Vec<Span>,
    has_emotes: bool,
}

fn calculate_relative_luminance(c: [f32; 4]) -> f32 {
    (0.2126 * c[0]) + (0.7152 * c[1]) + (0.0722 * c[2])
}

// TODO: revisit and fix
fn acc_name_to_color(acc_name: impl Hash, bg_rel_luminance: f32) -> [f32; 4] {
    let mut hasher = DefaultHasher::new();
    acc_name.hash(&mut hasher);
    let hash = hasher.finish();
    let r = (hash & 0xFF) as f32 / 255.0;
    let g = ((hash >> 8) & 0xFF) as f32 / 255.0;
    let b = ((hash >> 16) & 0xFF) as f32 / 255.0;
    let mut c = [r, g, b, 1.0];
    let luminance = calculate_relative_luminance(c);
    let (dark, bright, darken) = if bg_rel_luminance < luminance {
        (bg_rel_luminance, luminance, false)
    } else {
        (luminance, bg_rel_luminance, true)
    };
    let ratio = (bright + 0.05) / (dark + 0.05);
    if ratio <= 7.0 {
        let bright_prime = (7.0 * (dark + 0.05) - 0.05).min(1.0);
        let scaling_factor = if darken {
            bright_prime / bright
        } else {
            bright / bright_prime
        };
        c[0] = c[0] * scaling_factor;
        c[1] = c[1] * scaling_factor;
        c[2] = c[2] * scaling_factor;
    }
    c
}

const MAX_EMOTE_HEIGHT: f32 = 28.0;
fn get_emote_size(texture: &Texture) -> [f32; 2] {
    let aspect_ratio = texture.width as f32 / texture.height as f32;
    if texture.height as f32 > MAX_EMOTE_HEIGHT {
        [MAX_EMOTE_HEIGHT * aspect_ratio, MAX_EMOTE_HEIGHT]
    } else {
        [texture.width as f32, texture.height as f32]
    }
}

impl ProcessedMessage {
    fn new(chat: ChatMessageInfoOwned, emotes: &'static [Emote]) -> Self {
        let mut spans = vec![];
        let mut has_emotes = false;
        for word in chat.text.split_whitespace() {
            spans.push(
                if let Some(e) = emotes.iter().find(|em| em.matcher == word) {
                    has_emotes = true;
                    Span::Emote(EmoteSpan { emote: e })
                } else {
                    Span::Text(word.to_string())
                },
            );
        }
        Self {
            chat,
            spans,
            has_emotes,
        }
    }
    fn render_message(&self, ui: &Ui) {
        let bg_rel_luminance =
            calculate_relative_luminance(ui.style_color(nexus::imgui::StyleColor::WindowBg));
        let color = acc_name_to_color(&self.chat.account_name, bg_rel_luminance);
        let name = format!("{}:", self.chat.character_name);
        let name_size = ui.calc_text_size(&name);
        let font_size = ui.current_font_size();
        let _frame_pad_style = if self.has_emotes {
            let style = ui.push_style_var(StyleVar::FramePadding([
                0.0,
                0.5 * (MAX_EMOTE_HEIGHT - font_size),
            ]));
            ui.align_text_to_frame_padding();
            Some(style)
        } else {
            None
        };
        ui.text_colored(color, &name);
        let window_width = ui.window_content_region_width();
        let mut width = name_size[0];
        for span in &self.spans {
            let next_width = match span {
                Span::Emote(span) => {
                    if let Some(tex) =
                        get_texture_or_create_from_file(&span.emote.id, &span.emote.path)
                    {
                        let tex_size = get_emote_size(&tex);
                        tex_size[0]
                    } else {
                        ui.calc_text_size(&span.emote.matcher)[0]
                    }
                }
                Span::Text(text) => ui.calc_text_size(text)[0],
            } + ui.calc_text_size(" ")[0];
            if width + next_width < window_width {
                ui.same_line();
                ui.text(" ");
                ui.same_line();
            } else {
                width = 0.0;
                ui.align_text_to_frame_padding();
            }
            match span {
                Span::Emote(span) => {
                    if let Some(tex) = get_texture(&span.emote.id) {
                        Image::new(tex.id(), get_emote_size(&tex)).build(ui);
                    } else {
                        ui.text(&span.emote.matcher);
                    }
                }
                Span::Text(text) => {
                    ui.text(text);
                }
            }
            width += next_width;
        }
    }
}

#[derive(Debug, Clone)]
struct Emote {
    id: String,
    matcher: String,
    path: PathBuf,
}

impl Emote {
    fn load(path: PathBuf) -> Self {
        let id = path
            .file_stem()
            .expect("Emote file to have a stem")
            .to_string_lossy()
            .to_string();
        Self {
            id: format!("MEME_{}", id),
            matcher: id,
            path,
        }
    }
}

#[derive(Debug, Clone)]
struct EmoteSpan {
    emote: &'static Emote,
}

static MESSAGES: Mutex<Vec<ProcessedMessage>> = const { Mutex::new(Vec::new()) };
static EMOTES: OnceLock<&'static [Emote]> = OnceLock::new();

fn load_emotes(path: &impl AsRef<Path>) -> std::io::Result<Vec<Emote>> {
    let path = path.as_ref();
    if path.is_file() {
        panic!("Emotes must be a directory");
    }
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    let mut emotes = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        let extension = path.extension();
        // Only jpeg, png and gifs for now. stb_image does not support avif and webp
        if !["jpg", "jpeg", "png", "gif"]
            .iter()
            .any(|ext| extension.map_or(false, |e| e.to_ascii_lowercase() == *ext))
        {
            continue;
        }

        emotes.push(Emote::load(path));
    }

    Ok(emotes)
}

fn load() {
    log::info!("Loading Meme Message");
    let mut emotes = load_emotes(&emote_path()).expect("Failed to load emotes");
    emotes.shrink_to_fit();
    log::info!("Loaded ({}){:?}", emotes.len(), emotes);
    EMOTES
        .set(emotes.leak())
        .expect("EMOTES OnceLock to not already be initialized");
    register_render(RenderType::Render, render!(render_fn)).revert_on_unload();
    // register_render(RenderType::OptionsRender, render!(render_options)).revert_on_unload();
    // TODO: this event is not triggered, if you are already in a squad when logging in
    CHAT_MESSAGE
        .subscribe(event_consume!(|payload: Option<&RawChatMessageInfo>| {
            if let Some(payload) = payload {
                chat_message(payload.into());
            }
        }))
        .revert_on_unload();
}

fn render_fn(ui: &Ui) {
    Window::new("Squadchat").build(ui, || {
        let msgs = MESSAGES.lock().unwrap();
        for message in msgs.iter() {
            let _style = ui.push_style_var(StyleVar::ItemSpacing([0.0, 0.0]));
            message.render_message(ui);
        }
        let scroll = ui.scroll_y() == ui.scroll_max_y();
        if scroll {
            ui.set_scroll_here_y();
        }
    });
}

// TODO: probably wont be a OnceLock meme
// so we can reload emotes at runtime
// then we can also remove the unsafe, but we need to lock or pass the emotes around
fn unload() {
    // unleak the emotes
    let emotes = EMOTES.wait();
    unsafe {
        let _ = Vec::from_raw_parts(emotes.as_ptr() as *mut Emote, emotes.len(), emotes.len());
    }
}

fn chat_message(message: ChatMessageInfo<'_>) {
    let message = message.to_owned();
    let mut msgs = MESSAGES.lock().unwrap();
    msgs.push(ProcessedMessage::new(message, EMOTES.wait()));
}

nexus::export! {
    name: "Emote Chat",
    signature: -69423,
    flags: AddonFlags::None,
    load,
    unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/belst/nexus-emotes",
    log_filter: "warn,nexus_emotes=debug"
}
