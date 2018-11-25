extern crate futures;
extern crate reqwest;
extern crate serde_json;
extern crate serde;
extern crate rand;
extern crate openssl_probe;
#[macro_use]
extern crate serde_derive;
extern crate telegram_bot;
extern crate tokio_core;
extern crate time;
extern crate timer;
extern crate chrono;

use std::collections::HashMap;
use std::env;
use futures::Stream;
use serde_json::*;
use timer::Timer;
use reqwest::StatusCode;
use telegram_bot::*;
use tokio_core::reactor::Core;
use reqwest::Client;
use chrono::DateTime;
use std::ops::Add;
use chrono::Timelike;
use chrono::offset::Utc;
use time::Duration;

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

#[derive(Serialize, Deserialize, Clone, Debug)]
struct AccessToken {
    access_token: String
}

#[derive(Serialize, Deserialize)]
struct RetrieveRequest {
    consumer_key: String,
    access_token: String,
    detailType: String,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    chat_id: String,
    text: String,
}

enum AuthorizationState {
    WaitingForCallback(String),
    Authorized((AccessToken, Scheduling)),
}

struct Scheduling {
    at: DateTime<Utc>,
    period: Period,
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
    openssl_probe::init_ssl_cert_env_vars();
    let settings_file_name = "settings.txt";
    let token = env::var("TELEGRAM_BOT_API_TOKEN").unwrap();
    let consumer_key = env::var("POCKET_API_CONSUMER_KEY").unwrap();
    let (mut core, api) = build_api(&token);
    let redirect_uri = String::from("https://t.me/PocketReminderBot?start=send_me_to_proceed_next");
    let client = Client::new();
    let timer = Timer::new();
    let mut chat_states = HashMap::new();

    let previous_states = reschedule_from_file(settings_file_name, &timer, &client, &token, &consumer_key);
    for (chat_id, state) in previous_states {
        chat_states.insert(chat_id, state);
    }

    let future = api.stream().for_each(|update| {
        if let UpdateKind::Message(message) = update.kind {
            let maybe_state_update = match chat_states.get(&(message.chat.id())) {
                Some(AuthorizationState::Authorized((access_token, _))) => {
                    send_message(&client, &token, &(message.chat.id()), &String::from("Here is your random unread article!"));
                    send_random_unread_article(&client, &(message.chat.id()), &token, &consumer_key, access_token);
                    None
                }
                Some(AuthorizationState::WaitingForCallback(code)) =>
                    proceed_callback(&timer, &client, &consumer_key, &token, &(message.chat.id()), code, settings_file_name),
                None =>
                    init_auth(&client, &consumer_key, &token, &redirect_uri, &(message.chat.id()))
            };

            if let Some((chat_id, state)) = maybe_state_update {
                chat_states.insert(chat_id, state);
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

fn init_auth(client: &Client,
             consumer_key: &String,
             token: &String,
             redirect_uri: &String,
             chat_id: &ChatId) -> Option<(ChatId, AuthorizationState)> {
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
                send_message(client, token, chat_id, &String::from("Follow the link, verify the access, return back and press 'Start'!"));
                send_message(client, token, chat_id, &format!(
                    "https://getpocket.com/auth/authorize?request_token={}&redirect_uri={}",
                    initial_response.code,
                    redirect_uri));
                Some((*chat_id, AuthorizationState::WaitingForCallback(initial_response.code)))
            }
            Err(e) => {
                send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                eprintln!("Couldn't parse {} to InitialResponse, reason: {}", response.text().unwrap(), e);
                None
            }
        }
        Ok(response) => {
            send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Pocket API have not returned 200, status: {}", response.status());
            None
        }
        Err(e) => {
            send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Couldn't make /request request to Pocket API, reason: {}", e);
            None
        }
    }
}

fn proceed_callback(timer: &Timer,
                    client: &Client,
                    consumer_key: &String,
                    token: &String,
                    chat_id: &ChatId,
                    code: &String,
                    settings_file_name: &str) -> Option<(ChatId, AuthorizationState)> {
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
                    send_message(client, token, chat_id, &String::from("Here is your random unread article! Wait for next one after 24 hours or chat me at any time and I provide new one instantly."));
                    send_random_unread_article(client, chat_id, token, consumer_key, &access_token);
                    let scheduling = Scheduling {
                        at: Utc::now(),
                        period: Period::Day,
                    };
                    save_to_file(settings_file_name, chat_id, &access_token, &scheduling);
                    schedule_sending(timer, client, chat_id, token, consumer_key, &access_token, &scheduling);
                    Some((*chat_id, AuthorizationState::Authorized((access_token, scheduling))))
                }
                Err(e) => {
                    eprintln!("Couldn't parse {} to AccessToken, reason: {}", response.text().unwrap(), e);
                    send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                    None
                }
            }
        Ok(response) => {
            send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Pocket API have not returned 200, status: {}", response.status());
            None
        }
        Err(e) => {
            send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Couldn't make /authorize request to Pocket API, reason: {}", e);
            None
        }
    }
}

fn save_to_file(file_name: &str,
                chat_id: &ChatId,
                access_token: &AccessToken,
                scheduling: &Scheduling) -> () {
    use std::fs;
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(file_name)
        .unwrap();
    if let Err(e) = writeln!(file,
                             "{}::{}::{}::{}",
                             chat_id,
                             access_token.access_token,
                             scheduling.at.to_rfc2822(),
                             scheduling.period.to_string()) {
        eprintln!("Couldn't write scheduling to file {}, reason: {}", file_name, e);
    }
}

fn reschedule_from_file(file_name: &str,
                        timer: &Timer,
                        client: &Client,
                        token: &String,
                        consumer_key: &String) -> Vec<(ChatId, AuthorizationState)> {
    use std::fs;
    use std::convert::From;
    use std::str::FromStr;
    let content = fs::read_to_string(file_name).unwrap();
    let lines: Vec<&str> = content.split_terminator(|c: char| c == '\n').collect();
    let mut vec: Vec<(ChatId, AuthorizationState)> = vec![];
    for line in lines {
        let mut parts: Vec<&str> = line.split("::").collect();
        parts.reverse();
        let chat_id: ChatId = parts.pop().unwrap().parse::<i64>().unwrap().into();
        let access_token: AccessToken = AccessToken {
            access_token: String::from(parts.pop().unwrap())
        };
        let at = DateTime::from_utc(DateTime::parse_from_rfc2822(parts.pop().unwrap()).unwrap().naive_utc(), Utc);
        let period = Period::from_str(parts.pop().unwrap()).unwrap();
        let scheduling = Scheduling {
            at,
            period,
        };
        schedule_sending(timer, client, &chat_id, token, consumer_key, &access_token, &scheduling);
        vec.push((chat_id, AuthorizationState::Authorized((access_token, scheduling))));
    }
    vec
}

fn schedule_sending(timer: &Timer,
                    client: &Client,
                    chat_id: &ChatId,
                    token: &String,
                    consumer_key: &String,
                    access_token: &AccessToken,
                    scheduling: &Scheduling,
) -> () {
    let c = client.clone();
    let i = chat_id.clone();
    let t = token.clone();
    let k = consumer_key.clone();
    let a = access_token.clone();

    let now = Utc::now();
    let d = now
        .with_hour(scheduling.at.hour()).unwrap()
        .with_minute(scheduling.at.minute()).unwrap()
        .with_second(scheduling.at.second()).unwrap();
    let d = if d < now {
        d.add(Duration::days(1))
    } else {
        d
    };

    let g = timer.schedule(d, Some(scheduling.period.to_duration()), move || {
        send_random_unread_article(&c, &i, &t, &k, &a);
    });
    g.ignore();
}

fn send_random_unread_article(client: &Client,
                              chat_id: &ChatId,
                              token: &String,
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
                                    send_message(client, token, chat_id, &format!("https://getpocket.com/a/read/{}", *random_article_id));
                                }
                                _ => {
                                    send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                                    eprintln!("Couldn't parse 'list' to JSON object, 'list' is {}", v);
                                }
                            }
                        }
                        None => {
                            send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                            eprintln!("Couldn't find 'list' JSON object in response: {}", v);
                        }
                    }
                }
                Err(_) => {
                    send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
                    eprintln!("Couldn't parse {} to JSON", response.text().unwrap());
                }
            }
        ,
        Ok(response) => {
            send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Pocket API have not returned 200, status: {}", response.status());
        }
        Err(e) => {
            send_message(client, token, chat_id, &String::from("Some error occurred, chat @themirrortruth for help"));
            eprintln!("Couldn't make /get request to Pocket API, reason: {}", e);
        }
    }
}

fn send_message(client: &Client, token: &String, chat_id: &ChatId, text: &String) -> () {
    let chat_message = ChatMessage {
        chat_id: chat_id.to_string(),
        text: text.clone(),
    };
    let body = serde_json::to_string(&chat_message).unwrap();
    let request = client
        .post(format!("https://api.telegram.org/bot{}/sendMessage", token).as_str())
        .header("Content-Type", "application/json; charset=UTF8")
        .body(body);

    match request.send() {
        Ok(ref response) if response.status() != StatusCode::from_u16(200).unwrap() =>
            eprintln!("Telegram returned {} instead of 200", response.status().as_u16()),
        _ => ()
    };
}