use prost::Message;

use crate::vehicle_command::error::Result;
use crate::vehicle_command::proto::car_server::{
    action::ActionMsg,
    vehicle_action::VehicleActionMsg,
    vehicle_control_window_action::Action as WindowControlAction,
    Action, VehicleAction, VehicleControlWindowAction, Void,
};

pub fn build_window_action(vent: bool) -> Result<Vec<u8>> {
    let window_action = if vent {
        WindowControlAction::Vent(Void {})
    } else {
        WindowControlAction::Close(Void {})
    };

    let action = Action {
        action_msg: Some(ActionMsg::VehicleAction(VehicleAction {
            vehicle_action_msg: Some(VehicleActionMsg::VehicleControlWindowAction(
                VehicleControlWindowAction {
                    action: Some(window_action),
                },
            )),
        })),
    };
    let mut buf = Vec::new();
    action.encode(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vent_marshals_vehicle_control_window_action() {
        let payload = build_window_action(true).expect("marshal vent");
        let decoded = Action::decode(payload.as_slice()).expect("decode");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => {
                match vehicle_action.vehicle_action_msg {
                    Some(VehicleActionMsg::VehicleControlWindowAction(window)) => {
                        assert!(matches!(window.action, Some(WindowControlAction::Vent(_))));
                    }
                    other => panic!("unexpected action: {other:?}"),
                }
            }
            other => panic!("unexpected action msg: {other:?}"),
        }
    }

    #[test]
    fn close_marshals_vehicle_control_window_action() {
        let payload = build_window_action(false).expect("marshal close");
        let decoded = Action::decode(payload.as_slice()).expect("decode");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => {
                match vehicle_action.vehicle_action_msg {
                    Some(VehicleActionMsg::VehicleControlWindowAction(window)) => {
                        assert!(matches!(window.action, Some(WindowControlAction::Close(_))));
                    }
                    other => panic!("unexpected action: {other:?}"),
                }
            }
            other => panic!("unexpected action msg: {other:?}"),
        }
    }
}
