extern crate futures;
extern crate reqwest;
extern crate serde_json;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate telegram_bot;
extern crate tokio_core;

use std::collections::HashMap;
use std::env;
use futures::Stream;
use reqwest::StatusCode;
use telegram_bot::*;
use tokio_core::reactor::Core;

#[derive(Serialize)]
struct InitialRequest {
    consumer_key: String,
    redirect_uri: String,
}

#[derive(Deserialize)]
struct InitialResponse {
    code: String
}

#[derive(Serialize)]
struct AuthorizationRequest {
    consumer_key: String,
    code: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct AccessToken {
    access_token: String,
    username: String,
}

enum AuthorizationState {
    WaitingForCallback(String),
    Authorized(AccessToken),
}

fn main() {
    let mut core = Core::new().unwrap();

    let token = env::var("TELEGRAM_BOT_API_TOKEN").unwrap();
    let consumer_key = env::var("POCKET_API_CONSUMER_KEY").unwrap();
    let api = Api::configure(token).build(core.handle()).unwrap();
    let redirect_uri = String::from("https://t.me/PocketReminderBot");
    let mut user_states: HashMap<UserId, AuthorizationState> = HashMap::new();

    let future = api.stream().for_each(|update| {
        if let UpdateKind::Message(message) = update.kind {
            if let MessageKind::Text { ref data, .. } = message.kind {
                let maybe_user_state_update = match user_states.get(&message.from.id) {
                    Some(AuthorizationState::Authorized(_access_token)) => {
                        api.spawn(message.text_reply(format!("Authorized!")));
                        None
                    }
                    Some(AuthorizationState::WaitingForCallback(code)) => {
                        let authorization_request_struct = AuthorizationRequest {
                            consumer_key: consumer_key.to_owned(),
                            code: (*code).to_owned(),
                        };

                        let authorization_request_body = serde_json::to_string(&authorization_request_struct).unwrap();

                        let client = reqwest::Client::new();
                        let authorization_request = client.post("https://getpocket.com/v3/oauth/authorize")
                            .header("Content-Type", "application/json; charset=UTF8")
                            .header("X-Accept", "application/json")
                            .body(authorization_request_body);

                        match authorization_request.send() {
                            Ok(ref mut response) if response.status() == StatusCode::from_u16(200).unwrap() =>
                                match response.json::<AccessToken>() {
                                    Ok(access_token) => Some((message.from.id, AuthorizationState::Authorized(access_token))),
                                    _ => None
                                }
                            _ => None
                        }
                    }
                    None =>
                        {
                            let client = reqwest::Client::new();

                            let initial_request_struct = InitialRequest {
                                consumer_key: consumer_key.to_owned(),
                                redirect_uri: redirect_uri.to_owned(),
                            };
                            let initial_request_body = serde_json::to_string(&initial_request_struct).unwrap();

                            let initial_request = client
                                .post("https://getpocket.com/v3/oauth/request")
                                .header("Content-Type", "application/json; charset=UTF8")
                                .header("X-Accept", "application/json")
                                .body(initial_request_body);

                            match initial_request.send() {
                                Ok(mut response) => match response.json::<InitialResponse>() {
                                    Ok(initial_response) => {
                                        api.spawn(
                                            message.text_reply(
                                                format!(
                                                    "https://getpocket.com/auth/authorize?request_token={}&redirect_uri={}",
                                                    initial_response.code,
                                                    redirect_uri)
                                            ));
                                        api.spawn(message.text_reply(format!("Waiting for callback!")));
                                        Some((message.from.id, AuthorizationState::WaitingForCallback(initial_response.code)))
                                    }
                                    _ => None
                                }
                                _ => None
                            }
                        }
                };

                match maybe_user_state_update {
                    Some((user_id, state)) => user_states.insert(user_id, state),
                    None => None
                };
            }
        }

        Ok(())
    });

    core.run(future).unwrap();
}