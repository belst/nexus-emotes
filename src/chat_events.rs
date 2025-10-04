use core::ffi::c_char;
use core::mem::MaybeUninit;
use core::ptr;
use nexus::event::Event;
use std::ffi::CStr;
use std::fmt::Debug;
use time::{Date, Time, UtcDateTime};
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::FileTimeToSystemTime;

pub const SIGNATURE: i32 = 0x6777_0263;
pub const MESSAGE_IDENTIFIER: &str = "EV_CHAT:Message";

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MessageType {
    Error = 0,
    Map = 1,
    Party = 2,
    Squad = 3,
    Team = 4,
    Guild = 5,
    Whisper = 6,
    Local = 7,
}

impl From<u8> for MessageType {
    fn from(v: u8) -> MessageType {
        match v {
            1 => MessageType::Map,
            2 => MessageType::Party,
            3 => MessageType::Squad,
            4 => MessageType::Team,
            5 => MessageType::Guild,
            6 => MessageType::Whisper,
            7 => MessageType::Local,
            x => {
                log::warn!("Unknown message type: {}", x);
                MessageType::Error
            }
        }
    }
}

impl From<MessageType> for u8 {
    fn from(m: MessageType) -> u8 {
        m as u8
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct RawPlayer {
    pub character: *const c_char,
    pub account: *const c_char,
}

#[derive(Debug, Clone, Default)]
pub struct Player {
    pub character: Option<String>,
    pub account: Option<String>,
}

impl From<RawPlayer> for Player {
    fn from(p: RawPlayer) -> Self {
        Self {
            character: unsafe {
                p.character.as_ref().map(|c| {
                    CStr::from_ptr(c as *const c_char)
                        .to_str()
                        .expect("Invalid UTF-8")
                        .to_string()
                })
            },
            account: unsafe {
                p.account.as_ref().map(|c| {
                    CStr::from_ptr(c as *const c_char)
                        .to_str()
                        .expect("Invalid UTF-8")
                        .to_string()
                })
            },
        }
    }
}

impl Default for RawPlayer {
    fn default() -> Self {
        RawPlayer {
            character: ptr::null(),
            account: ptr::null(),
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GuildFlags {
    None = 0,
    IsActive = 1 << 0,
}

impl From<u8> for GuildFlags {
    fn from(v: u8) -> GuildFlags {
        match v {
            1 => GuildFlags::IsActive,
            _ => GuildFlags::None,
        }
    }
}

impl From<GuildFlags> for u8 {
    fn from(f: GuildFlags) -> u8 {
        f as u8
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct GuildSource {
    pub source: RawPlayer,
    pub index: u8,
    pub flags: GuildFlags,
}

impl Default for GuildSource {
    fn default() -> Self {
        GuildSource {
            source: RawPlayer::default(),
            index: 0,
            flags: GuildFlags::None,
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WhisperFlags {
    None = 0,
    SourceIsInterlocutor = 1 << 0,
}

impl From<u8> for WhisperFlags {
    fn from(v: u8) -> WhisperFlags {
        match v {
            1 => WhisperFlags::SourceIsInterlocutor,
            _ => WhisperFlags::None,
        }
    }
}

impl From<WhisperFlags> for u8 {
    fn from(f: WhisperFlags) -> u8 {
        f as u8
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct WhisperSource {
    pub interlocutor: RawPlayer,
    pub flags: WhisperFlags,
}

impl Default for WhisperSource {
    fn default() -> Self {
        WhisperSource {
            interlocutor: RawPlayer::default(),
            flags: WhisperFlags::None,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TeamSource {
    pub source: RawPlayer,
    pub map_id: u32,
}

impl Default for TeamSource {
    fn default() -> Self {
        TeamSource {
            source: RawPlayer::default(),
            map_id: 0,
        }
    }
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum SquadFlags {
    #[default]
    None = 0,
    IsBroadcast = 1 << 0,
    SourceIsCommander = 1 << 1,
    SourceIsLeutanant = 1 << 2,
}

impl From<u8> for SquadFlags {
    fn from(v: u8) -> SquadFlags {
        match v {
            1 => SquadFlags::IsBroadcast,
            2 => SquadFlags::SourceIsCommander,
            4 => SquadFlags::SourceIsLeutanant,
            _ => SquadFlags::None,
        }
    }
}

impl From<SquadFlags> for u8 {
    fn from(f: SquadFlags) -> u8 {
        f as u8
    }
}

#[derive(Debug)]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct RawSquadSource {
    pub source: RawPlayer,
    pub flags: SquadFlags,
}

impl Default for RawSquadSource {
    fn default() -> Self {
        RawSquadSource {
            source: RawPlayer::default(),
            flags: SquadFlags::None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SquadSource {
    pub source: Player,
    pub flags: SquadFlags,
}

impl From<RawSquadSource> for SquadSource {
    fn from(raw: RawSquadSource) -> Self {
        Self {
            source: raw.source.into(),
            flags: raw.flags,
        }
    }
}

/// The union of possible sources. This mirrors the C++ anonymous union.
/// Use `unsafe` to access fields.
#[repr(C)]
#[derive(Copy, Clone)]
pub union MessageUnion {
    pub player_source: RawPlayer,
    pub guild: GuildSource,
    pub team: TeamSource,
    pub whisper: WhisperSource,
    pub squad: RawSquadSource,
}

impl Default for MessageUnion {
    fn default() -> Self {
        // zero-initialize
        unsafe { MaybeUninit::<MessageUnion>::zeroed().assume_init() }
    }
}

impl Debug for MessageUnion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageUnion")
            // .field("player_source", unsafe { &self.player_source })
            // .field("guild", unsafe { &self.guild })
            // .field("team", unsafe { &self.team })
            // .field("whisper", unsafe { &self.whisper })
            // .field("squad", unsafe { &self.squad })
            .finish()
    }
}

pub enum MessageSource {
    Map(Player),
    Party(Player),
    Player(Player),
    Local(Player),
    Guild(GuildSource),
    Team(TeamSource),
    Whisper(WhisperSource),
    Squad(SquadSource),
    Error,
}

impl MessageSource {
    pub fn from_raw(source: MessageUnion, r#type: MessageType) -> Self {
        unsafe {
            use MessageType as MT;
            match r#type {
                MT::Map => Self::Map(Player::from(source.player_source)),
                MT::Party => Self::Party(Player::from(source.player_source)),
                MT::Squad => Self::Squad(source.squad.into()),
                MT::Team => Self::Team(source.team),
                MT::Guild => Self::Guild(source.guild),
                MT::Whisper => Self::Whisper(source.whisper),
                MT::Local => Self::Local(Player::from(source.player_source)),
                MT::Error => Self::Error,
            }
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct RawMessage {
    pub content: *const c_char,
    pub timestamp: FILETIME,
    pub r#type: u8,
    // padding here (3 bytes), I assume repr(C) adds it
    // pub _pad: [u8; 3],
    pub source: MessageUnion,
}

impl Default for RawMessage {
    fn default() -> Self {
        RawMessage {
            content: ptr::null(),
            timestamp: FILETIME::default(),
            r#type: 0,
            // _pad: [0; 3],
            source: MessageUnion::default(),
        }
    }
}

pub struct Message {
    pub content: Option<String>,
    pub timestamp: UtcDateTime,
    pub source: MessageSource,
}

fn timestamp_to_date_time(timestamp: &FILETIME) -> anyhow::Result<UtcDateTime> {
    let mut systemtime = SYSTEMTIME::default();
    unsafe {
        if let Err(e) = FileTimeToSystemTime(timestamp as *const _, &mut systemtime as *mut _) {
            return Err(e.into());
        }
    }
    Ok(UtcDateTime::new(
        Date::from_calendar_date(
            systemtime.wYear as i32,
            unsafe { std::mem::transmute(systemtime.wMonth as u8) }, // transmute because converting to
            // enum is annoying
            systemtime.wDay as u8,
        )?,
        Time::from_hms_milli(
            systemtime.wHour as u8,
            systemtime.wMinute as u8,
            systemtime.wSecond as u8,
            systemtime.wMilliseconds as u16,
        )?,
    ))
}

impl From<&RawMessage> for Message {
    fn from(msg: &RawMessage) -> Self {
        Self {
            content: unsafe {
                msg.content.as_ref().map(|c| {
                    CStr::from_ptr(c as *const c_char)
                        .to_str()
                        .expect("Invalid UTF-8")
                        .to_string()
                })
            },
            timestamp: timestamp_to_date_time(&msg.timestamp).unwrap_or_else(|e| {
                log::error!("Failed to convert timestamp: {}", e);
                UtcDateTime::now()
            }),
            source: MessageSource::from_raw(msg.source, msg.r#type.into()),
        }
    }
}

pub const CHAT_MESSAGE: Event<RawMessage> = unsafe { Event::new(MESSAGE_IDENTIFIER) };
