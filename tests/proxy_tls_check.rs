use std::path::Path;

use lazytesla::api::FleetClient;

#[tokio::test]
async fn fleet_client_verifies_proxy_tls_strictly() {
    let cert_path = format!("{}/config/tls-cert.pem", env!("CARGO_MANIFEST_DIR"));
    let cert_path = cert_path.as_str();
    if !Path::new(cert_path).exists() {
        return;
    }

    let client = FleetClient::with_tls("https://127.0.0.1:4443".into(), Some(cert_path))
        .expect("with_tls should build with add_root_certificate");

    let response = client
        .send_command("test", "auto_conditioning_start", "test-token", true)
        .await;

    // Proxy running: auth/vehicle errors are fine; connection/TLS must succeed.
    match response {
        Ok(()) => {}
        Err(err) => {
            let message = err.to_string();
            assert!(
                !message.contains("could not connect to command proxy"),
                "unexpected connect failure: {message}"
            );
            assert!(
                !message.contains("TLS error connecting to command proxy"),
                "unexpected TLS failure: {message}"
            );
            assert!(
                !message.contains("CA:TRUE"),
                "regenerate tls-cert.pem as a server certificate: {message}"
            );
        }
    }

    let http = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(cert_path).unwrap()).unwrap())
        .build()
        .unwrap();
    let res = http
        .post("https://127.0.0.1:4443/api/1/vehicles/test/command/auto_conditioning_start")
        .header("Authorization", "Bearer test")
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("strict TLS reqwest should connect to local proxy");
    assert!(res.status().as_u16() > 0);
}