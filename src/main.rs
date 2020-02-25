extern crate ws;

#[macro_use]
extern crate lazy_static;

extern crate reqwest;
extern crate tubepeek_server_rust;

mod db_connection;
use db_connection::{PgPool, establish_connection};

mod ws_dto;
use ws_dto::*;
use serde::{Deserialize, Serialize};

use ws::{listen, Handler, Sender, Message, CloseCode};
use ws::Result as WsResult;

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{Value as JsonValue, Error};
use diesel::PgConnection;
use diesel::prelude::*;

use tubepeek_server_rust::models::{Usermaster, NewUser, NewSocialIdentity, SocialIdentity};
use chrono::{Utc, NaiveDateTime};
use std::ptr::null;
// use serde_json::Result as JsResult;



//use tubepeek_server_rust::schema::{usermaster};

// Using lazy static to have a global reference to my connection pool
// However, I feel that for testing/mocking this won't be great.
lazy_static! {
    static ref POOL: PgPool = { establish_connection() };
    static ref WS_CONNECTED_CLIENTS: Mutex<HashMap<u32, WsConnectedClientMetadata>> = Mutex::new(HashMap::new());
}

pub struct WsConnectedClientMetadata {
    pub socketId: u32,
    // pub socket : &'a Sender,
    pub googleUserId: String,
    pub currentVideo: Option<WsConnectedClientCurrentVideo>,
}

pub struct WsConnectedClientCurrentVideo {
    pub videoUrl: String,
    pub title: String,
    pub thumbnail_url: String,
}


struct WsServer {
    out: Sender
}

pub enum WsMessageType {
    TakeMySocialIdentity,
    UserChangedOnlineStatus,
    AddThisPersonToMyFriendsList,
    ChangedVideo
}

impl Handler for WsServer {
    fn on_message(&mut self, msg: Message) -> WsResult<()> {
        let raw_message = msg.into_text().unwrap();
        let json = &raw_message[..];
        println!("The message from the client is {:#?}", json);

        let get_json_value = || -> Result<JsonValue, Error> {
            let v: JsonValue = serde_json::from_str(json)?;
            Ok(v)
        };

        let v: Result<JsonValue, Error> = get_json_value();

        if let Err(_err) = v {
            return self.out.send("Invalid json value")
        }

        let v: JsonValue = v.unwrap();

        let pool = POOL.clone();
        let database_connection = pool.get().expect("Failed to get pooled connection"); // Not sure when a panic is triggered here

        let response = match v["messageType"].as_str().unwrap() {
            "TakeMySocialIdentity" => handle_social_identity(json, &database_connection, &self.out),
            "UserChangedOnlineStatus" => handle_user_online_status_change(json, &database_connection),
            "AddThisPersonToMyFriendsList" => handle_frend_addition(json, &database_connection),
            "ChangedVideo" => handle_vidoe_change(json, &database_connection, &self.out),
            _ => "Unknown message type. ".to_owned(),
        };

        // Message::Text(raw_message)
        self.out.send(response)
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        let client_conn_id = self.out.connection_id();

        let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();
        let conn_metadata_maybe = connected_clients.get(&client_conn_id);

        match conn_metadata_maybe {
            Some(conn_metadata) => {
                connected_clients.remove(&client_conn_id);
            },
            _ => println!("Don't panic"),
        };
        match code {
            CloseCode::Normal => println!("The client is done with the connection."),
            CloseCode::Away   => println!("The client is leaving the site."),
            _ => println!("The client encountered an error: {}", reason),
        }
    }
}

fn handle_social_identity(json : &str, connection: &PgConnection, ws_client: &Sender) -> String {
    println!("Got TakeMySocialIdentity message.");
    let social_identity_maybe: Result<TakeSocialIdentityMessage, Error> = serde_json::from_str(json);

    use tubepeek_server_rust::schema::usermaster::dsl::*;
    use tubepeek_server_rust::schema::social_identities::dsl::*;

    match social_identity_maybe {
        Ok(social_identity) => {
            let now = Utc::now().naive_utc();
            let auth_data_email = social_identity.authData.emailAddress.as_str();
            let google_user_id = social_identity.authData.googleUserId.as_str();

            let save_social_identity = |user_record_id: i64, auth_data_email: &str, prov: String, auth_data: &AuthData, now: &NaiveDateTime|  {
                let new_social_identity = NewSocialIdentity {
                    user_id: user_record_id,
                    provider: prov,
                    email_address: auth_data_email,
                    full_name: auth_data.fullName.as_str(),
                    uid: auth_data.googleUserId.as_str(),
                    image_url: auth_data.imageUrl.as_str(),
                    created_at: *now,
                };

                let new_social_id_db_record = diesel::insert_into(social_identities)
                    .values(&new_social_identity)
                    .execute(connection)
                    .expect("Error saving new social identity");
            };

            let existing_user_results = usermaster.filter(tubepeek_server_rust::schema::usermaster::dsl::email_address.eq(&social_identity.authData.emailAddress))
                .limit(1)
                .load::<Usermaster>(connection)
                .expect("Error loading users");

            if existing_user_results.len() > 0 {
                let existing_social_identity = social_identities
                    .filter(tubepeek_server_rust::schema::social_identities::dsl::user_id.eq(existing_user_results[0].id)
                        .and(tubepeek_server_rust::schema::social_identities::dsl::provider.eq(&social_identity.provider)))
                    .load::<SocialIdentity>(connection)
                    .expect("Error loading user social identity");

                if(existing_social_identity.len() > 0) {
                    diesel::update(social_identities.filter(tubepeek_server_rust::schema::social_identities::dsl::id.eq(existing_social_identity[0].id)))
                        .set((
                            tubepeek_server_rust::schema::social_identities::dsl::full_name.eq(&social_identity.authData.fullName),
                            tubepeek_server_rust::schema::social_identities::dsl::image_url.eq(&social_identity.authData.imageUrl),
                            tubepeek_server_rust::schema::social_identities::dsl::updated_at.eq(&now))
                        )
                        .execute(connection);
                } else {
                    save_social_identity(existing_user_results[0].id, auth_data_email, social_identity.provider, &social_identity.authData, &now);
                }
            } else {
                let new_user = NewUser {
                    email_address: auth_data_email,
                    created_at: now,
                };

                let new_user_db_record = diesel::insert_into(usermaster)
                    .values(&new_user)
                    .get_result::<Usermaster>(connection)
                    .expect("Error saving new user");

                save_social_identity(new_user_db_record.id, auth_data_email, social_identity.provider, &social_identity.authData, &now);
            }

            let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();
            connected_clients.insert(ws_client.connection_id(), WsConnectedClientMetadata {
                socketId: ws_client.connection_id(),
                // socket: ws_client,
                googleUserId: google_user_id.to_owned(),
                currentVideo: None,
            });

            return "all good".to_owned()
        },
        Err(err_msg) => {
            println!("Invalid take social identity.");
            "bad".to_owned()
        }
    };

    "All good".to_owned()
}

fn handle_user_online_status_change(json : &str, connection: &PgConnection) -> String {
    println!("Got UserChangedOnlineStatus message.");

    "All good".to_owned()
}

fn handle_frend_addition(json : &str, connection: &PgConnection) -> String {
    println!("Got AddThisPersonToMyFriendsList message.");

    "All good".to_owned()
}

fn handle_vidoe_change(json : &str, connection: &PgConnection, ws_client: &Sender) -> String {
    println!("Got ChangedVideo message.");
    let video_change_maybe: Result<VideoChangeMessage, Error> = serde_json::from_str(json);

    use tubepeek_server_rust::schema::usermaster::dsl::*;
    use tubepeek_server_rust::schema::social_identities::dsl::*;

    match video_change_maybe {
        Ok(video_change) => {
            let video_url = video_change.videoUrl.as_str();
            let google_user_id = video_change.googleUserId.as_str();
            let youtube_query_url = format!("http://www.youtube.com{}{}", "/oembed?format=json&url=", video_url);

            println!("youtube_query_url: {}", youtube_query_url);

            let youtube_response_maybe = reqwest::blocking::get(youtube_query_url.as_str());
            match youtube_response_maybe {
                Ok(valid_response) => {
                    let decoded_video_details = valid_response.json::<YoutubeVideoResponse>();

                    println!("decoded_video_details {:#?}", decoded_video_details);
                },
                Err(err_msg) => {
                    println!("Invalid video change.");
                }
            }
        },
        Err(err_msg) => {
            println!("Invalid video change.");
        }
    };
    "All good".to_owned()
}


fn main() {
    println!("Tubepeek server up and running ...");

    listen("127.0.0.1:9160", |out| WsServer { out: out } ).unwrap()

//    listen("127.0.0.1:9160", |out| {
//        move |msg| {
//            let conn_id : u32 = out.connection_id();
//
//            println!("Connection id: {:?}", conn_id);
//            println!("Received messaage: {:?}", msg);
//
//            out.send(msg)
//        }
//    }).unwrap()
}
