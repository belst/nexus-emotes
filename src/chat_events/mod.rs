// allow unused because we will refactor this out into its own crate at one point
#![allow(unused)]
#[allow(warnings)]
pub mod raw;
use core::str;
use std::ffi::CStr;

use anyhow::Context;
use nexus::event::Event;
use raw::GW2_CHAT_EVENT;
use raw::Message as RawMessage;
use time::{Date, Time, UtcDateTime};
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Time::FileTimeToSystemTime;

use crate::chat_events::raw::GloballyUniqueIdentifier;

impl Default for GloballyUniqueIdentifier {
    fn default() -> Self {
        Self {
            Data1: Default::default(),
            Data2: Default::default(),
            Data3: Default::default(),
            Data4: Default::default(),
        }
    }
}

const CHAT_EVENT_IDENTIFIER: &str = const {
    unsafe {
        match CStr::from_bytes_with_nul_unchecked(GW2_CHAT_EVENT).to_str() {
            Ok(s) => s,
            Err(e) => unreachable!(),
        }
    }
};

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

#[derive(Debug, Clone, Default)]
pub struct GenericMessage {
    pub account: GloballyUniqueIdentifier,
    pub character_name: String,
    pub account_name: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GameEmote {
    Bless,
    Beckon,
    Dance,
    Sit,
    Yes,
    No,
    Cower,
    Laugh,
    Other(u32),
}

#[derive(Debug, Clone)]
pub enum MessageSource {
    Guild {
        message: GenericMessage,
        guild_index: u32,
    },
    GuildMotD {
        content: String,
        guild_index: u32,
    },
    Local(GenericMessage),
    Map(GenericMessage),
    Party(GenericMessage),
    Squad(GenericMessage),
    SquadMessage(String),
    TeamPvP(GenericMessage),
    TeamWvW {
        message: GenericMessage,
        map_id: u32,
    },
    Whisper(GenericMessage),
    Emote {
        character_name: Option<String>,
        action_taken: GameEmote,
    },
    EmoteCustom {
        character_name: Option<String>,
        action_taken: String,
    },
}

unsafe fn rawstr_to_string(raw: raw::StringUTF8) -> Result<Option<String>, str::Utf8Error> {
    if raw.is_null() {
        Ok(None)
    } else {
        unsafe { Ok(Some(CStr::from_ptr(raw).to_str()?.to_owned())) }
    }
}

/// Safety: DO NOT USE WITH EMOTE TYPE
unsafe fn union_to_generic(
    raw: &raw::Message__bindgen_ty_1,
) -> Result<GenericMessage, anyhow::Error> {
    // TODO: this is kinda unsafe, because we use the `Local` union variant, but the
    // layout of all union variants except for Emote should start the same
    // Only Guild and TeamWvW have an extra u32 at the end
    unsafe {
        let account = raw.Local.Account;
        let character_name = rawstr_to_string(raw.Local.CharacterName)
            .context("character_name")?
            .unwrap_or_default();
        let account_name = rawstr_to_string(raw.Local.AccountName).context("account_name")?;
        let content = rawstr_to_string(raw.Local.Content)?.unwrap_or_default();
        Ok(GenericMessage {
            account,
            character_name,
            account_name,
            content,
        })
    }
}

impl MessageSource {
    pub fn from_raw(raw: &RawMessage) -> anyhow::Result<Self> {
        match raw.Type {
            raw::MessageType_Error => Err(anyhow::anyhow!("Error message type")),
            raw::MessageType_Guild => unsafe {
                let message = union_to_generic(&raw.__bindgen_anon_1)?;
                Ok(Self::Guild {
                    message,
                    guild_index: raw.__bindgen_anon_1.Guild.GuildIndex,
                })
            },
            raw::MessageType_GuildMotD => unsafe {
                let content =
                    rawstr_to_string(raw.__bindgen_anon_1.GuildMotD.Content)?.unwrap_or_default();
                Ok(Self::GuildMotD {
                    content,
                    guild_index: raw.__bindgen_anon_1.GuildMotD.GuildIndex,
                })
            },
            raw::MessageType_Local => unsafe {
                union_to_generic(&raw.__bindgen_anon_1).map(Self::Local)
            },
            raw::MessageType_Map => unsafe {
                union_to_generic(&raw.__bindgen_anon_1).map(Self::Map)
            },
            raw::MessageType_Party => unsafe {
                union_to_generic(&raw.__bindgen_anon_1).map(Self::Party)
            },
            raw::MessageType_Squad => unsafe {
                union_to_generic(&raw.__bindgen_anon_1).map(Self::Squad)
            },
            raw::MessageType_SquadMessage => unsafe {
                let content =
                    rawstr_to_string(raw.__bindgen_anon_1.SquadMessage)?.unwrap_or_default();
                Ok(Self::SquadMessage(content))
            },
            raw::MessageType_SquadBroadcast => unsafe {
                // TODO: double check if broadcast is the same as squad message
                let content =
                    rawstr_to_string(raw.__bindgen_anon_1.SquadMessage)?.unwrap_or_default();
                Ok(Self::SquadMessage(content))
            },
            raw::MessageType_TeamPvP => unsafe {
                union_to_generic(&raw.__bindgen_anon_1).map(Self::TeamPvP)
            },
            raw::MessageType_TeamWvW => unsafe {
                let message = union_to_generic(&raw.__bindgen_anon_1)?;
                Ok(Self::TeamWvW {
                    message,
                    map_id: raw.__bindgen_anon_1.TeamWvW.Map,
                })
            },
            raw::MessageType_Whisper => unsafe {
                union_to_generic(&raw.__bindgen_anon_1).map(Self::Whisper)
            },
            raw::MessageType_Emote => unsafe {
                let character_name = rawstr_to_string(raw.__bindgen_anon_1.Emote.CharacterName)?;
                let action_taken = match raw.__bindgen_anon_1.Emote.ActionTaken {
                    raw::EmoteType_Bless => GameEmote::Bless,
                    raw::EmoteType_Beckon => GameEmote::Beckon,
                    raw::EmoteType_Dance => GameEmote::Dance,
                    raw::EmoteType_Sit => GameEmote::Sit,
                    raw::EmoteType_Yes => GameEmote::Yes,
                    raw::EmoteType_No => GameEmote::No,
                    raw::EmoteType_Cower => GameEmote::Cower,
                    raw::EmoteType_Laugh => GameEmote::Laugh,
                    n => GameEmote::Other(n),
                };
                Ok(Self::Emote {
                    character_name,
                    action_taken,
                })
            },
            raw::MessageType_EmoteCustom => unsafe {
                let character_name = rawstr_to_string(raw.__bindgen_anon_1.Emote.CharacterName)?;
                let action_taken = rawstr_to_string(raw.__bindgen_anon_1.EmoteCustom.ActionTaken)?
                    .unwrap_or_default();
                Ok(Self::EmoteCustom {
                    character_name,
                    action_taken,
                })
            },
            n => Err(anyhow::anyhow!("Unknown message type: {n}")),
        }
    }

    pub fn content(&self) -> Option<&str> {
        match self {
            Self::Guild { message, .. } | Self::TeamWvW { message, .. } => Some(&message.content),
            Self::GuildMotD { content, .. } => Some(content),
            Self::SquadMessage(content) => Some(content),
            Self::Local(message)
            | Self::Map(message)
            | Self::Party(message)
            | Self::Squad(message)
            | Self::TeamPvP(message)
            | Self::Whisper(message) => Some(&message.content),
            // TODO: emotes on custom emotes?
            Self::Emote { .. } | Self::EmoteCustom { .. } => None,
        }
    }
}

// TODO: flags
pub struct Message {
    pub timestamp: UtcDateTime,
    pub source: MessageSource,
}

impl Message {
    pub fn content(&self) -> Option<&str> {
        self.source.content()
    }
}

impl TryFrom<RawMessage> for Message {
    type Error = anyhow::Error;

    fn try_from(value: RawMessage) -> Result<Self, Self::Error> {
        let time: FILETIME = FILETIME {
            dwLowDateTime: value.DateTime.Low,
            dwHighDateTime: value.DateTime.High,
        };
        let timestamp = timestamp_to_date_time(&time)?;
        let source = MessageSource::from_raw(&value)?;
        Ok(Self { timestamp, source })
    }
}

pub const CHAT_MESSAGE: Event<RawMessage> = unsafe { Event::new(CHAT_EVENT_IDENTIFIER) };
