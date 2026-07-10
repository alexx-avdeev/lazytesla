pub mod climate;
pub mod client;
pub mod crypto;
pub mod error;
pub mod fleet;
pub mod session;

pub use client::VehicleCommandClient;
pub use error::VehicleCommandError;

pub mod proto {
    pub mod car_server {
        include!(concat!(env!("OUT_DIR"), "/car_server.rs"));
    }
    pub mod errors {
        include!(concat!(env!("OUT_DIR"), "/errors.rs"));
    }
    pub mod keys {
        include!(concat!(env!("OUT_DIR"), "/keys.rs"));
    }
    pub mod managed_charging {
        include!(concat!(env!("OUT_DIR"), "/managed_charging.rs"));
    }
    pub mod signatures {
        include!(concat!(env!("OUT_DIR"), "/signatures.rs"));
    }
    pub mod universal_message {
        include!(concat!(env!("OUT_DIR"), "/universal_message.rs"));
    }
    pub mod vcsec {
        include!(concat!(env!("OUT_DIR"), "/vcsec.rs"));
    }
}