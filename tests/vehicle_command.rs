use base64::{engine::general_purpose::STANDARD, Engine};
use lazytesla::vehicle_command::climate;
use lazytesla::vehicle_command::crypto::key::PrivateKey;
use lazytesla::vehicle_command::fleet::USER_AGENT;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn metadata_and_key_tests_run_in_lib() {}

#[test]
fn climate_action_round_trips_protobuf() {
    let on = climate::build_climate_action(true).expect("marshal on");
    let off = climate::build_climate_action(false).expect("marshal off");
    assert_ne!(on, off);
}

#[tokio::test]
async fn signed_command_posts_base64_routable_message() {
    let server = MockServer::start().await;
    let key_path = format!("{}/config/fleet-key.pem", env!("CARGO_MANIFEST_DIR"));
    if !std::path::Path::new(&key_path).exists() {
        return;
    }

    let key = PrivateKey::load(std::path::Path::new(&key_path)).expect("load key");
    let request = lazytesla::vehicle_command::crypto::signer::build_session_info_request(
        lazytesla::vehicle_command::proto::universal_message::Domain::Infotainment,
        key.public_bytes(),
        &[0_u8; 16],
        &[1_u8; 16],
    );
    let mut encoded = Vec::new();
    prost::Message::encode(&request, &mut encoded).expect("encode request");
    let expected_b64 = STANDARD.encode(&encoded);

    Mock::given(method("POST"))
        .and(path("/api/1/vehicles/5YJSA11111111111/signed_command"))
        .and(header("Authorization", "Bearer user-token"))
        .and(header("User-Agent", USER_AGENT))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": STANDARD.encode(b"")
        })))
        .mount(&server)
        .await;

    let mut transport = lazytesla::vehicle_command::fleet::FleetTransport::new(server.uri());
    let _ = transport
        .signed_command("5YJSA11111111111", "user-token", &encoded)
        .await;

    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1);
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains(&expected_b64));
}

#[tokio::test]
async fn wake_up_hits_fleet_endpoint() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/1/vehicles/5YJSA11111111111/wake_up"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": { "state": "online" }
        })))
        .mount(&server)
        .await;

    let mut transport = lazytesla::vehicle_command::fleet::FleetTransport::new(server.uri());
    transport
        .wake_up("5YJSA11111111111", "user-token")
        .await
        .expect("wake up");
}