use nexus::arcdps::extras::{ChannelType, ChatMessageInfoOwned};
use time::{Duration, UtcDateTime};

use crate::chat_events::{GenericMessage, Message, MessageSource};

impl From<ChatMessageInfoOwned> for Message {
    fn from(info: ChatMessageInfoOwned) -> Self {
        Self {
            timestamp: UtcDateTime::from_unix_timestamp(
                info.timestamp.to_utc().to_utc().timestamp(),
            )
            .expect("timestamp SHOULD always be valid")
                + Duration::nanoseconds(info.timestamp.to_utc().timestamp_subsec_nanos() as i64),
            source: match info.channel_type {
                ChannelType::Party => MessageSource::Party(GenericMessage {
                    character_name: info.character_name,
                    account_name: Some(info.account_name),
                    content: info.text,
                    ..Default::default()
                }),
                ChannelType::Squad => MessageSource::Squad(GenericMessage {
                    character_name: info.character_name,
                    account_name: Some(info.account_name),
                    content: info.text,
                    ..Default::default()
                }),
                ChannelType::Reserved => todo!("What does reserved mean?"),
                ChannelType::Invalid => todo!("What does invalid mean?"),
            },
        }
    }
}
