use nexus::arcdps::extras::message::{ChatMessageInfo, ChatMessageInfoOwned, RawChatMessageInfo};
use nexus::event_consume;
use nexus::gui::{register_render, render, RenderType};
use nexus::imgui::{Image, Ui, Window};
use nexus::paths::get_addon_dir;
use nexus::texture::get_texture_or_create_from_file;
use nexus::{event::extras::CHAT_MESSAGE, AddonFlags, UpdateProvider};
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
}

fn acc_name_to_color(acc_name: impl Hash) -> [f32; 4] {
    let mut hasher = DefaultHasher::new();
    acc_name.hash(&mut hasher);
    let hash = hasher.finish();
    let r = (hash & 0xFF) as f32 / 255.0;
    let g = ((hash >> 8) & 0xFF) as f32 / 255.0;
    let b = ((hash >> 16) & 0xFF) as f32 / 255.0;
    [r, g, b, 1.0]
}

impl ProcessedMessage {
    fn new(chat: ChatMessageInfoOwned, emotes: &'static [Emote]) -> Self {
        let mut spans = vec![];
        log::info!("Processing message: {}", chat.text);
        for word in chat.text.split_whitespace() {
            log::info!("Processing word: {}", word);
            spans.push(
                if let Some(e) = emotes.iter().find(|em| em.matcher == word) {
                    Span::Emote(EmoteSpan { emote: e })
                } else {
                    Span::Text(word.to_string())
                },
            );
        }
        Self { chat, spans }
    }
    fn render_message(&self, ui: &Ui) {
        let color = acc_name_to_color(&self.chat.account_name);
        ui.text_colored(color, format!("{}:", self.chat.character_name));
        for span in &self.spans {
            ui.same_line();
            ui.text(" ");
            ui.same_line();
            match span {
                Span::Emote(span) => {
                    if let Some(tex) =
                        get_texture_or_create_from_file(&span.emote.id, &span.emote.path)
                    {
                        Image::new(tex.id(), [tex.width as f32, tex.height as f32]).build(ui);
                    } else {
                        ui.text(&span.emote.matcher);
                    }
                }
                Span::Text(text) => ui.text(text),
            }
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
    EMOTES.set(emotes.leak());
    register_render(RenderType::Render, render!(render_fn)).revert_on_unload();
    // register_render(RenderType::OptionsRender, render!(render_options)).revert_on_unload();
    CHAT_MESSAGE
        .subscribe(event_consume!(|payload: Option<&RawChatMessageInfo>| {
            log::info!("Received ChatMessage meme before conversion: {:?}", payload);
            if let Some(payload) = payload {
                chat_message(payload.into());
            }
        }))
        .revert_on_unload();
}

fn render_fn(ui: &Ui) {
    Window::new("Meme").build(ui, || {
        ui.text("Squadchat:");
        let msgs = MESSAGES.lock().unwrap();
        for message in msgs.iter() {
            message.render_message(ui);
        }
    });
}

fn unload() {
    // unleak the emotes
    let emotes = EMOTES.wait();
    unsafe {
        let _ = Vec::from_raw_parts(emotes.as_ptr() as *mut Emote, emotes.len(), emotes.len());
    }
}

fn chat_message(message: ChatMessageInfo<'_>) {
    log::info!("Received ChatMessage meme");
    log::debug!("Callback thread: {:?}", std::thread::current().id());
    let message = message.to_owned();
    log::debug!("{:?}", message);
    let mut msgs = MESSAGES.lock().unwrap();
    msgs.push(ProcessedMessage::new(message, EMOTES.wait()));
}

nexus::export! {
    name: "Meme Message",
    signature: -69423,
    flags: AddonFlags::None,
    load,
    unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/belst/nexus-mememessage",
    log_filter: "warn,mememessage=debug"
}
