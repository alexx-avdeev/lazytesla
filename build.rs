use std::io::Result;

fn main() -> Result<()> {
    unsafe {
        std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().expect("protoc"));
    }

    let protos = [
        "proto/universal_message.proto",
        "proto/signatures.proto",
        "proto/car_server.proto",
        "proto/common.proto",
        "proto/vehicle.proto",
        "proto/vcsec.proto",
        "proto/errors.proto",
        "proto/keys.proto",
        "proto/managed_charging.proto",
    ];

    prost_build::Config::new().compile_protos(&protos, &["proto"])?;

    println!("cargo:rerun-if-changed=proto/");
    Ok(())
}