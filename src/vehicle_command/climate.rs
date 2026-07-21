use prost::Message;

use crate::vehicle_command::error::Result;
use crate::vehicle_command::proto::car_server::{
    action::ActionMsg,
    hvac_temperature_adjustment_action::{self, temperature::Type as TemperatureType},
    vehicle_action::VehicleActionMsg,
    Action, HvacAutoAction, HvacTemperatureAdjustmentAction, VehicleAction, Void,
};

pub fn build_set_temp_action(driver_celsius: f32, passenger_celsius: f32) -> Result<Vec<u8>> {
    let action = Action {
        action_msg: Some(ActionMsg::VehicleAction(VehicleAction {
            vehicle_action_msg: Some(VehicleActionMsg::HvacTemperatureAdjustmentAction(
                HvacTemperatureAdjustmentAction {
                    driver_temp_celsius: driver_celsius,
                    passenger_temp_celsius: passenger_celsius,
                    level: Some(hvac_temperature_adjustment_action::Temperature {
                        r#type: Some(TemperatureType::TempMax(Void {})),
                    }),
                    hvac_temperature_zone: Vec::new(),
                    ..Default::default()
                },
            )),
        })),
    };
    let mut buf = Vec::new();
    action.encode(&mut buf)?;
    Ok(buf)
}

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
    fn set_temp_marshals_hvac_temperature_adjustment_action() {
        let payload = build_set_temp_action(22.0, 22.0).expect("marshal set temp");
        let decoded = Action::decode(payload.as_slice()).expect("decode action");
        match decoded.action_msg {
            Some(ActionMsg::VehicleAction(vehicle_action)) => {
                match vehicle_action.vehicle_action_msg {
                    Some(VehicleActionMsg::HvacTemperatureAdjustmentAction(hvac)) => {
                        assert!((hvac.driver_temp_celsius - 22.0).abs() < f32::EPSILON);
                        assert!((hvac.passenger_temp_celsius - 22.0).abs() < f32::EPSILON);
                    }
                    other => panic!("unexpected action: {other:?}"),
                }
            }
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