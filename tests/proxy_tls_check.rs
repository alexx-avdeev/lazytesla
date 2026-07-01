use std::path::Path;

use lazytesla::api::FleetClient;

#[tokio::test]
async fn fleet_client_connects_to_local_proxy() {
    let cert_path = "/Users/axel/Development/Learning/lazytesla/config/tls-cert.pem";
    if !Path::new(cert_path).exists() {
        return;
    }

    let client = FleetClient::with_tls("https://127.0.0.1:4443".into(), Some(cert_path))
        .expect("with_tls should build");

    let url = "https://127.0.0.1:4443/api/1/vehicles/test/command/auto_conditioning_start";
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
        }
    }

    // Sanity: direct reqwest with danger_accept_invalid_certs reaches the proxy.
    let http = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .build()
        .unwrap();
    let res = http
        .post(url)
        .header("Authorization", "Bearer test")
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .expect("reqwest should connect to local proxy");
    assert!(res.status().as_u16() > 0);
}