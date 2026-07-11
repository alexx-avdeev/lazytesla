use prost::Message;

use crate::vehicle_command::error::Result;
use crate::vehicle_command::proto::vcsec::{
    unsigned_message::SubMessage, RkeActionE, UnsignedMessage,
};

pub fn build_rke_action(lock: bool) -> Result<Vec<u8>> {
    let action = if lock {
        RkeActionE::RkeActionLock
    } else {
        RkeActionE::RkeActionUnlock
    };

    let message = UnsignedMessage {
        sub_message: Some(SubMessage::RkeAction(action as i32)),
    };

    let mut buf = Vec::new();
    message.encode(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_command::proto::vcsec::UnsignedMessage;

    #[test]
    fn lock_marshals_rke_action_lock() {
        let payload = build_rke_action(true).expect("marshal lock");
        let decoded = UnsignedMessage::decode(payload.as_slice()).expect("decode");
        match decoded.sub_message {
            Some(SubMessage::RkeAction(action)) => {
                assert_eq!(action, RkeActionE::RkeActionLock as i32);
            }
            other => panic!("unexpected sub_message: {other:?}"),
        }
    }

    #[test]
    fn unlock_marshals_rke_action_unlock() {
        let payload = build_rke_action(false).expect("marshal unlock");
        let decoded = UnsignedMessage::decode(payload.as_slice()).expect("decode");
        match decoded.sub_message {
            Some(SubMessage::RkeAction(action)) => {
                assert_eq!(action, RkeActionE::RkeActionUnlock as i32);
            }
            other => panic!("unexpected sub_message: {other:?}"),
        }
    }
}