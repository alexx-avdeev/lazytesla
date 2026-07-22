use prost::Message;

use crate::vehicle_command::error::Result;
use crate::vehicle_command::proto::car_server::{
    action::ActionMsg, vehicle_action::VehicleActionMsg, Action, ChargingSetLimitAction,
    VehicleAction,
};

pub fn build_set_charge_limit_action(percent: i32) -> Result<Vec<u8>> {
    let action = Action {
        action_msg: Some(ActionMsg::VehicleAction(VehicleAction {
            vehicle_action_msg: Some(VehicleActionMsg::ChargingSetLimitAction(
                ChargingSetLimitAction { percent },
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
    fn set_charge_limit_marshals_charging_set_limit_action() {
        let payload = build_set_charge_limit_action(80).expect("marshal set charge limit");
        let decoded = Action::decode(payload.as_slice()).expect("decode action");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => {
                match vehicle_action.vehicle_action_msg {
                    Some(VehicleActionMsg::ChargingSetLimitAction(action)) => {
                        assert_eq!(action.percent, 80);
                    }
                    other => panic!("unexpected action: {other:?}"),
                }
            }
            other => panic!("unexpected action msg: {other:?}"),
        }
    }
}
