use lazytesla::api::{FleetApi, FleetClient};
use lazytesla::auth::partner::PartnerAuth;
use lazytesla::config::Config;
use reqwest::Client;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_config(base_url: &str, domain: Option<&str>) -> Config {
    Config {
        client_id: "test-client".into(),
        client_secret: "test-secret".into(),
        redirect_uri: "http://localhost:8484/callback".into(),
        audience: base_url.into(),
        callback_port: 8484,
        domain: domain.map(str::to_string),
    }
}

fn mock_fleet_api(mock: &MockServer) -> FleetApi {
    let base = mock.uri();
    let config = test_config(&base, Some("example.com"));
    FleetApi::with_clients(
        FleetClient::with_http(base.clone(), Client::new()),
        PartnerAuth::with_options(
            config,
            Client::new(),
            format!("{base}/oauth2/v3/token"),
        ),
    )
}

fn vehicles_response() -> serde_json::Value {
    serde_json::json!({
        "response": [{
            "id_s": "12345678901234567",
            "vin": "5YJSA11111111111",
            "display_name": "Nikola 2.0",
            "state": "online",
            "in_service": false
        }],
        "count": 1
    })
}

async fn mock_partner_token(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/oauth2/v3/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "partner-token",
            "token_type": "Bearer",
            "expires_in": 3600
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn list_vehicles_returns_parsed_vehicles() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/1/vehicles"))
        .and(header("Authorization", "Bearer user-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vehicles_response()))
        .mount(&server)
        .await;

    let client = FleetClient::with_http(server.uri(), Client::new());
    let vehicles = client
        .list_vehicles("user-token")
        .await
        .expect("vehicle list should succeed");

    assert_eq!(vehicles.len(), 1);
    assert_eq!(vehicles[0].display_name, "Nikola 2.0");
    assert_eq!(vehicles[0].vin, "5YJSA11111111111");
    assert_eq!(vehicles[0].state, "online");
}

#[tokio::test]
async fn register_partner_posts_domain() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/1/partner_accounts"))
        .and(header("Authorization", "Bearer partner-token"))
        .and(body_string_contains("example.com"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": { "account_id": "registered" }
        })))
        .mount(&server)
        .await;

    let client = FleetClient::with_http(server.uri(), Client::new());
    client
        .register_partner("partner-token", "example.com")
        .await
        .expect("partner registration should succeed");
}

#[tokio::test]
async fn partner_token_uses_client_credentials() {
    let server = MockServer::start().await;
    mock_partner_token(&server).await;

    let config = test_config(&server.uri(), Some("example.com"));
    let partner = PartnerAuth::with_options(
        config,
        Client::new(),
        format!("{}/oauth2/v3/token", server.uri()),
    );

    let token = partner
        .partner_token()
        .await
        .expect("partner token request should succeed");

    assert_eq!(token, "partner-token");
}

#[tokio::test]
async fn fetch_vehicles_registers_then_lists() {
    let server = MockServer::start().await;
    mock_partner_token(&server).await;

    Mock::given(method("POST"))
        .and(path("/api/1/partner_accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": { "account_id": "registered" }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/1/vehicles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vehicles_response()))
        .mount(&server)
        .await;

    let api = mock_fleet_api(&server);
    let config = test_config(&server.uri(), Some("example.com"));

    let vehicles = api
        .fetch_vehicles(&config, "user-token")
        .await
        .expect("fetch should register and list vehicles");

    assert_eq!(vehicles.len(), 1);
    assert_eq!(vehicles[0].display_name, "Nikola 2.0");
}

#[tokio::test]
async fn fetch_vehicles_returns_config_error_without_domain() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/1/vehicles"))
        .respond_with(ResponseTemplate::new(412).set_body_json(serde_json::json!({
            "error": "account_not_registered",
            "error_description": "Account must be registered in the current region"
        })))
        .mount(&server)
        .await;

    let config = test_config(&server.uri(), None);
    let api = FleetApi::from_config(&config);

    let err = api
        .fetch_vehicles(&config, "user-token")
        .await
        .expect_err("missing domain should return config error");

    assert!(err.to_string().contains("TESLA_DOMAIN"));
}

#[tokio::test]
async fn fetch_vehicles_retries_list_after_registration_error() {
    let server = MockServer::start().await;
    mock_partner_token(&server).await;

    Mock::given(method("POST"))
        .and(path("/api/1/partner_accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": { "account_id": "registered" }
        })))
        .expect(2)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/1/vehicles"))
        .respond_with(
            ResponseTemplate::new(412).set_body_json(serde_json::json!({
                "error": "account_not_registered",
                "error_description": "Account must be registered in the current region"
            })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/1/vehicles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(vehicles_response()))
        .mount(&server)
        .await;

    let api = mock_fleet_api(&server);
    let config = test_config(&server.uri(), Some("example.com"));

    let vehicles = api
        .fetch_vehicles(&config, "user-token")
        .await
        .expect("fetch should retry after registration error");

    assert_eq!(vehicles.len(), 1);
}