use nexus::arcdps::extras::{ChannelType, ChatMessageInfoOwned};

use crate::chat_events::{Message, MessageSource, Player, RawSquadSource, SquadFlags, SquadSource};

pub struct ChatMessage {
    pub content: String,
    pub source: MessageSource,
}

impl From<ChatMessageInfoOwned> for ChatMessage {
    fn from(info: ChatMessageInfoOwned) -> Self {
        Self {
            content: info.text,
            source: match info.channel_type {
                ChannelType::Party => MessageSource::Party(Player {
                    character: Some(info.character_name),
                    account: Some(info.account_name),
                }),
                ChannelType::Squad => MessageSource::Squad(SquadSource {
                    source: Player {
                        character: Some(info.character_name),
                        account: Some(info.account_name),
                    },
                    flags: if info.is_broadcast {
                        SquadFlags::IsBroadcast
                    } else {
                        SquadFlags::None
                    },
                }),
                ChannelType::Reserved => todo!("What does reserved mean?"),
                ChannelType::Invalid => todo!("What does invalid mean?"),
            },
        }
    }
}

impl From<Message> for ChatMessage {
    fn from(message: Message) -> Self {
        Self {
            content: message.content.unwrap_or("".into()),
            source: message.source,
        }
    }
}

