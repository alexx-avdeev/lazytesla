use prost::Message;

use crate::vehicle_command::error::Result;
use crate::vehicle_command::proto::car_server::{
    action::ActionMsg, vehicle_action::VehicleActionMsg, Action, HvacAutoAction, VehicleAction,
};

pub fn build_climate_action(power_on: bool) -> Result<Vec<u8>> {
    let action = Action {
        action_msg: Some(ActionMsg::VehicleAction(VehicleAction {
            vehicle_action_msg: Some(VehicleActionMsg::HvacAutoAction(HvacAutoAction {
                power_on,
                manual_override: false,
            })),
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
    fn climate_on_marshals_hvac_auto_action() {
        let payload = build_climate_action(true).expect("marshal climate on");
        assert!(!payload.is_empty());
        let decoded = Action::decode(payload.as_slice()).expect("decode action");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => match vehicle_action.vehicle_action_msg {
                Some(VehicleActionMsg::HvacAutoAction(hvac)) => assert!(hvac.power_on),
                other => panic!("unexpected action: {other:?}"),
            },
            other => panic!("unexpected action msg: {other:?}"),
        }
    }

    #[test]
    fn climate_off_marshals_power_on_false() {
        let payload = build_climate_action(false).expect("marshal climate off");
        let decoded = Action::decode(payload.as_slice()).expect("decode action");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => match vehicle_action.vehicle_action_msg {
                Some(VehicleActionMsg::HvacAutoAction(hvac)) => assert!(!hvac.power_on),
                other => panic!("unexpected action: {other:?}"),
            },
            other => panic!("unexpected action msg: {other:?}"),
        }
    }
}