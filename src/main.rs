extern crate futures;
extern crate reqwest;
extern crate serde_json;
extern crate serde;
extern crate rand;
extern crate timer;
extern crate time;
#[macro_use]
extern crate serde_derive;
extern crate telegram_bot;
extern crate tokio_core;

use std::collections::HashMap;
use std::env;
use futures::Stream;
use timer::Timer;
use timer::Guard;
use time::Duration;
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

#[derive(Serialize, Deserialize, Clone)]
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
    Authorized(AccessToken, Guard),
}

enum Period {
    Minute,
    Hour,
    ThreeHours,
    SixHours,
    TwelveHours,
    Day,
}

impl std::str::FromStr for Period {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, <Self as std::str::FromStr>::Err> {
        match s {
            "minute" => Ok(Period::Minute),
            "hour" => Ok(Period::Hour),
            "three_hours" => Ok(Period::ThreeHours),
            "six_hours" => Ok(Period::SixHours),
            "twelve_hours" => Ok(Period::TwelveHours),
            "day" => Ok(Period::Day),
            _ => Err(())
        }
    }
}

impl ToString for Period {
    fn to_string(&self) -> String {
        match self {
            Period::Minute => String::from("minute"),
            Period::Hour => String::from("hour"),
            Period::ThreeHours => String::from("three_hours"),
            Period::SixHours => String::from("six_hours"),
            Period::TwelveHours => String::from("twelve_hours"),
            Period::Day => String::from("day")
        }
    }
}

impl Period {
    fn to_duration(&self) -> Duration {
        match self {
            Period::Minute => Duration::minutes(1),
            Period::Hour => Duration::hours(1),
            Period::ThreeHours => Duration::hours(3),
            Period::SixHours => Duration::hours(6),
            Period::TwelveHours => Duration::hours(12),
            Period::Day => Duration::hours(24)
        }
    }
}

fn main() {
    let token = env::var("TELEGRAM_BOT_API_TOKEN").unwrap();
    let consumer_key = env::var("POCKET_API_CONSUMER_KEY").unwrap();
    let (mut core, api) = build_api(&token);
    let redirect_uri = String::from("https://t.me/PocketReminderBot?start=send_me_to_proceed_next");
    let client = reqwest::Client::new();
    let mut user_states: HashMap<UserId, AuthorizationState> = HashMap::new();

    let future = api.stream().for_each(|update| {
        if let UpdateKind::Message(message) = update.kind {
            let maybe_user_state_update = match user_states.get(&message.from.id) {
                Some(AuthorizationState::Authorized(ref access_token, _)) => {
                    send_message(&api, &(message.from.id), &String::from("Here is your random unread article!"));
                    send_random_unread_article(&api, &client, &(message.from.id), &consumer_key, access_token);
                    None
                }
                Some(AuthorizationState::WaitingForCallback(code)) =>
                    proceed_callback(&client, &token, &consumer_key, &(message.from.id), &api, code),
                None =>
                    init_auth(&client, &consumer_key, &redirect_uri, &(message.from.id), &api)
            };

            if let Some((user_id, state)) = maybe_user_state_update {
                user_states.insert(user_id, state);
            }
        }
        Ok(())
    });

    core.run(future).unwrap();
}

fn build_api(token: &String) -> (Core, Api) {
    let core = Core::new().unwrap();
    let api = Api::configure(token).build(core.handle()).unwrap();
    (core, api)
}

fn init_auth(client: &reqwest::Client,
             consumer_key: &String,
             redirect_uri: &String,
             user_id: &UserId,
             api: &Api) -> Option<(UserId, AuthorizationState)> {
    let initial_request_struct = InitialRequest {
        consumer_key: consumer_key.clone(),
        redirect_uri: redirect_uri.clone(),
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
                send_message(api, user_id, &String::from("Follow the link, verify the access, return back and press 'Start'!"));
                send_message(api, user_id, &format!(
                    "https://getpocket.com/auth/authorize?request_token={}&redirect_uri={}",
                    initial_response.code,
                    redirect_uri));
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
                    token: &String,
                    consumer_key: &String,
                    user_id: &UserId,
                    api: &Api,
                    code: &String) -> Option<(UserId, AuthorizationState)> {
    let authorization_request_struct = AuthorizationRequest {
        consumer_key: consumer_key.clone(),
        code: (*code).clone(),
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
                    send_message(api, user_id, &String::from("Here is your random unread article! Wait for next one after 24 hours or chat me at any time and I provide new one instantly."));
                    send_random_unread_article(api, client, user_id, consumer_key, &access_token);
                    let guard = schedule_sending(token, user_id, consumer_key, &access_token, Period::Minute);
                    Some((*user_id, AuthorizationState::Authorized(access_token, guard)))
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

fn schedule_sending(token: &String,
                    user_id: &UserId,
                    consumer_key: &String,
                    access_token: &AccessToken,
                    period: Period,
) -> Guard {
    let timer = Timer::new();
    let token_cloned = token.clone();
    let user_id_cloned = user_id.clone();
    let consumer_key_cloned = consumer_key.clone();
    let access_token_cloned = access_token.clone();
    let guard = timer.schedule_repeating(
        period.to_duration(),
        move || {
            let (_core, api) = build_api(&token_cloned);
            let client = reqwest::Client::new();
            send_random_unread_article(&api, &client, &user_id_cloned, &consumer_key_cloned, &access_token_cloned);
        });
    guard
}

fn send_random_unread_article(api: &Api,
                              client: &reqwest::Client,
                              user_id: &UserId,
                              consumer_key: &String,
                              access_token: &AccessToken) -> () {
    let request_data = RetrieveRequest {
        consumer_key: consumer_key.clone(),
        access_token: access_token.access_token.clone(),
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
                                    send_message(&api, &user_id, &format!("https://getpocket.com/a/read/{}", *random_article_id));
                                }
                                _ => {
                                    send_message(&api, &user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                                    eprintln!("Couldn't parse 'list' to JSON object, 'list' is {}", v);
                                }
                            }
                        }
                        None => {
                            send_message(&api, &user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                            eprintln!("Couldn't find 'list' JSON object in response: {}", v);
                        }
                    }
                }
                Err(_) => {
                    send_message(&api, &user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                    eprintln!("Couldn't parse {} to JSON", response.text().unwrap());
                }
            }
        ,
        Ok(response) => {
            send_message(&api, &user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Pocket API have not returned 200, status: {}", response.status());
        }
        Err(e) => {
            send_message(&api, &user_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Couldn't make /get request to Pocket API, reason: {}", e);
        }
    }
}

fn send_message(api: &Api, user_id: &UserId, text: &String) -> () {
    api.spawn(user_id.text(text));
}