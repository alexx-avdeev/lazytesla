use prost::Message;

use crate::vehicle_command::error::Result;
use crate::vehicle_command::proto::car_server::{
    action::ActionMsg,
    charging_start_stop_action::ChargingAction,
    vehicle_action::VehicleActionMsg,
    Action, ChargingSetLimitAction, ChargingStartStopAction, VehicleAction, Void,
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

pub fn build_charge_start_stop_action(start: bool) -> Result<Vec<u8>> {
    let charging_action = if start {
        ChargingAction::Start(Void {})
    } else {
        ChargingAction::Stop(Void {})
    };

    let action = Action {
        action_msg: Some(ActionMsg::VehicleAction(VehicleAction {
            vehicle_action_msg: Some(VehicleActionMsg::ChargingStartStopAction(
                ChargingStartStopAction {
                    charging_action: Some(charging_action),
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

    #[test]
    fn charge_start_marshals_charging_start_stop_action() {
        let payload = build_charge_start_stop_action(true).expect("marshal start");
        let decoded = Action::decode(payload.as_slice()).expect("decode");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => {
                match vehicle_action.vehicle_action_msg {
                    Some(VehicleActionMsg::ChargingStartStopAction(action)) => {
                        assert!(matches!(
                            action.charging_action,
                            Some(ChargingAction::Start(_))
                        ));
                    }
                    other => panic!("unexpected action: {other:?}"),
                }
            }
            other => panic!("unexpected action msg: {other:?}"),
        }
    }

    #[test]
    fn charge_stop_marshals_charging_start_stop_action() {
        let payload = build_charge_start_stop_action(false).expect("marshal stop");
        let decoded = Action::decode(payload.as_slice()).expect("decode");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => {
                match vehicle_action.vehicle_action_msg {
                    Some(VehicleActionMsg::ChargingStartStopAction(action)) => {
                        assert!(matches!(
                            action.charging_action,
                            Some(ChargingAction::Stop(_))
                        ));
                    }
                    other => panic!("unexpected action: {other:?}"),
                }
            }
            other => panic!("unexpected action msg: {other:?}"),
        }
    }
}
