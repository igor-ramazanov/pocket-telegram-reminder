extern crate futures;
extern crate reqwest;
extern crate serde_json;
extern crate serde;
extern crate rand;
#[macro_use]
extern crate serde_derive;
extern crate telegram_bot;
extern crate tokio_core;

use std::collections::HashMap;
use std::env;
use futures::Stream;
use serde_json::*;
use reqwest::StatusCode;
use telegram_bot::*;
use tokio_core::reactor::Core;

#[derive(Serialize, Deserialize)]
struct InitialRequest {
    consumer_key: String,
    redirect_uri: String,
}

#[derive(Serialize, Deserialize)]
struct InitialResponse {
    code: String
}

#[derive(Serialize, Deserialize)]
struct AuthorizationRequest {
    consumer_key: String,
    code: String,
}

#[derive(Serialize, Deserialize)]
struct AccessToken {
    access_token: String,
    username: String,
}

#[derive(Serialize, Deserialize)]
struct RetrieveRequest {
    consumer_key: String,
    access_token: String,
    detailType: String,
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
    let redirect_uri = String::from("https://t.me/PocketReminderBot?start=send_me_to_proceed_next");
    let client = reqwest::Client::new();
    let mut user_states: HashMap<UserId, AuthorizationState> = HashMap::new();

    let future = api.stream().for_each(|update| {
        if let UpdateKind::Message(message) = update.kind {
            let maybe_user_state_update = match user_states.get(&message.from.id) {
                Some(AuthorizationState::Authorized(access_token)) => {
                    send_random_unread_article(&api, &(message.from.id), &client, &consumer_key, &access_token);
                    None
                }
                Some(AuthorizationState::WaitingForCallback(code)) =>
                    proceed_callback(&client, &consumer_key, &(message.from.id), &api, code),
                None =>
                    init_auth(&client, &consumer_key, &redirect_uri, &(message.from.id), &api)
            };

            match maybe_user_state_update {
                Some((user_id, state)) => user_states.insert(user_id, state),
                None => None
            };
        }

        Ok(())
    });

    core.run(future).unwrap();
}

fn init_auth(client: &reqwest::Client,
             consumer_key: &String,
             redirect_uri: &String,
             user_id: &UserId,
             api: &Api) -> Option<(UserId, AuthorizationState)> {
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
        Ok(ref mut response) if response.status() == StatusCode::from_u16(200).unwrap() => match response.json::<InitialResponse>() {
            Ok(initial_response) => {
                send_message(api, user_id, &format!(
                    "https://getpocket.com/auth/authorize?request_token={}&redirect_uri={}",
                    initial_response.code,
                    redirect_uri));
                send_message(api, user_id, &String::from("Click on the link below, verify the bot and then press 'Start'!"));
                Some((*user_id, AuthorizationState::WaitingForCallback(initial_response.code)))
            }
            Err(e) => {
                send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                eprintln!("Couldn't parse {} to InitialResponse, reason: {}", response.text().unwrap(), e);
                None
            }
        }
        Ok(response) => {
            send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Pocket API have not returned 200, status: {}", response.status());
            None
        }
        Err(e) => {
            send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Couldn't make /request request to Pocket API, reason: {}", e);
            None
        }
    }
}

fn proceed_callback(client: &reqwest::Client,
                    consumer_key: &String,
                    user_id: &UserId,
                    api: &Api,
                    code: &String) -> Option<(UserId, AuthorizationState)> {
    let authorization_request_struct = AuthorizationRequest {
        consumer_key: consumer_key.to_owned(),
        code: (*code).to_owned(),
    };

    let authorization_request_body = serde_json::to_string(&authorization_request_struct).unwrap();
    let authorization_request = client.post("https://getpocket.com/v3/oauth/authorize")
        .header("Content-Type", "application/json; charset=UTF8")
        .header("X-Accept", "application/json")
        .body(authorization_request_body);

    match authorization_request.send() {
        Ok(ref mut response) if response.status() == StatusCode::from_u16(200).unwrap() =>
            match response.json::<AccessToken>() {
                Ok(access_token) => {
                    send_random_unread_article(api, user_id, client, consumer_key, &access_token);
                    Some((*user_id, AuthorizationState::Authorized(access_token)))
                }
                Err(e) => {
                    eprintln!("Couldn't parse {} to AccessToken, reason: {}", response.text().unwrap(), e);
                    send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                    None
                }
            }
        Ok(response) => {
            send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Pocket API have not returned 200, status: {}", response.status());
            None
        }
        Err(e) => {
            send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Couldn't make /authorize request to Pocket API, reason: {}", e);
            None
        }
    }
}

fn send_random_unread_article(api: &Api,
                              user_id: &UserId,
                              client: &reqwest::Client,
                              consumer_key: &String,
                              access_token: &AccessToken) -> () {
    let request_data = RetrieveRequest {
        consumer_key: consumer_key.to_owned(),
        access_token: access_token.access_token.to_owned(),
        detailType: String::from("simple"),
    };

    match client.post("https://getpocket.com/v3/get")
        .header("Content-Type", "application/json; charset=UTF8")
        .header("X-Accept", "application/json")
        .body(serde_json::to_string(&request_data).unwrap())
        .send() {
        Ok(ref mut response) if response.status() == StatusCode::from_u16(200).unwrap() =>
            match response.json::<Value>() {
                Ok(v) => {
                    match v.get("list") {
                        Some(v) => {
                            match v {
                                Value::Object(map) => {
                                    let keys: Vec<&String> = map.keys().collect();
                                    let random_number = rand::random::<i64>();
                                    let random_article_id = keys[((random_number as usize) % keys.len())];
                                    send_message(api, user_id, &format!("https://getpocket.com/a/read/{}", *random_article_id));
                                }
                                _ => {
                                    send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                                    eprintln!("Couldn't parse 'list' to JSON object, 'list' is {}", v);
                                }
                            }
                        }
                        None => {
                            send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                            eprintln!("Couldn't find 'list' JSON object in response: {}", v);
                        }
                    }
                }
                Err(_) => {
                    send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                    eprintln!("Couldn't parse {} to JSON", response.text().unwrap());
                }
            }
        ,
        Ok(response) => {
            send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Pocket API have not returned 200, status: {}", response.status());
        }
        Err(e) => {
            send_message(api, user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Couldn't make /get request to Pocket API, reason: {}", e);
        }
    }
}

fn send_message(api: &Api, user_id: &UserId, text: &String) -> () {
    api.spawn(user_id.text(text));
}