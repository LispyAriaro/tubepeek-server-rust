extern crate ws;

#[macro_use]
extern crate lazy_static;
extern crate reqwest;
extern crate regex;

extern crate tubepeek_server_rust;

mod db_connection;
use db_connection::{establish_connection, PgPool};

mod ws_dto;
use ws_dto::*;

mod utils;
use utils::*;

use ws::Result as WsResult;
use ws::{listen, CloseCode, Handler, Message, Sender};

use std::collections::HashMap;
use std::sync::Mutex;

use diesel::prelude::*;
use diesel::PgConnection;
use serde_json::{json, Error, Value as JsonValue};

use chrono::{NaiveDateTime, Utc};
use tubepeek_server_rust::models::{NewSocialIdentity, NewUser, NewUserFriend, SocialIdentity, Usermaster, Video, NewVideo, UserVideo, NewUserVideo};


// Using lazy static to have a global reference to my connection pool
// However, I feel that for testing/mocking this won't be great.
lazy_static! {
    static ref POOL: PgPool = { establish_connection() };
    static ref WS_CONNECTED_CLIENTS: Mutex<HashMap<u32, WsConnectedClientMetadata>> =
        Mutex::new(HashMap::new());
}

#[derive(Debug)]
pub struct WsConnectedClientMetadata {
    pub socketId: u32,
    pub socket: Sender,
    pub googleUserId: String,
    pub currentVideo: Option<WsConnectedClientCurrentVideo>,
    pub onlineFriends: Box<Vec<WsOnlineFriend>>,
}

#[derive(Debug)]
pub struct WsConnectedClientCurrentVideo {
    pub videoUrl: String,
    pub title: String,
    pub thumbnail_url: String,
}

#[derive(Debug)]
pub struct WsOnlineFriend {
    pub socketId: u32,
    pub googleUserId: String,
}

struct WsServer {
    out: Sender,
}

// USELESS FOR THE MOMENT. COULDN'T FIND A SIMPLE WAY TO GET A STRING REPRESENTATION OF AN ENUM VALUE
// TO USE IN A MATCH STATEMENT. IT HAS TO BE A STRING BECAUSE WHAT I AM COMPARING TO IS A STRING COMING FROM THE CLIENT SIDE
// OF THE WEB SOCKET CONNECTION.
pub enum WsMessageType {
    TakeMySocialIdentity,
    UserChangedOnlineStatus,
    AddThisPersonToMyFriendsList,
    ChangedVideo,
}

impl Handler for WsServer {
    fn on_message(&mut self, msg: Message) -> WsResult<()> {
        let raw_message = msg.into_text().unwrap();
        println!("The message from the client is {:#?}", &raw_message);

        let get_json_value = || -> Result<JsonValue, Error> {
            let v: JsonValue = serde_json::from_str(&raw_message)?;
            Ok(v)
        };

        let json_maybe: Result<JsonValue, Error> = get_json_value();
        if let Err(_err) = json_maybe {
            return self.out.send("Invalid json value");
        }

        let pool = POOL.clone();
        let database_connection = pool.get().expect("Failed to get pooled connection");

        let response = match json_maybe.unwrap()["messageType"].as_str().unwrap() {
            "TakeMySocialIdentity" => {
                handle_social_identity(&raw_message, &database_connection, &self.out)
            }
            "UserChangedOnlineStatus" => {
                handle_user_online_status_change(&raw_message, &database_connection)
            }
            "AddThisPersonToMyFriendsList" => {
                handle_frend_addition(&raw_message, &database_connection)
            }
            "ChangedVideo" => handle_vidoe_change(&raw_message, &database_connection, &self.out),
            _ => "Unknown message type".to_owned(),
        };
        self.out.send(response)
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        let client_conn_id = self.out.connection_id();

        let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();
        let conn_metadata_maybe = connected_clients.get(&client_conn_id);

        match conn_metadata_maybe {
            Some(conn_metadata) => {
                let broadcast_data = json!({
                    "action": "TakeFriendOnlineStatus",
                    "googleUserId": conn_metadata.googleUserId,
                    "onlineState": false
                });

                for friend in conn_metadata.onlineFriends.iter() {
                    let friend_conn_maybe: Option<&WsConnectedClientMetadata> =
                        connected_clients.get(&friend.socketId);

                    match friend_conn_maybe {
                        Some(conn) => {
                            conn.socket.send(broadcast_data.to_string());
                        },
                        None => println!("Done not")
                    };
                }

                connected_clients.remove(&client_conn_id);
            }
            _ => println!("Don't panic"),
        };
        match code {
            CloseCode::Normal => println!("The client is done with the connection."),
            CloseCode::Away => println!("The client is leaving the site."),
            _ => println!("The client encountered an error: {}", reason),
        }
    }
}

fn handle_social_identity(json: &str, connection: &PgConnection, ws_client: &Sender) -> String {
    let social_identity_maybe: Result<TakeSocialIdentityMessage, Error> =
        serde_json::from_str(json);

    use tubepeek_server_rust::schema::social_identities::dsl::*;
    use tubepeek_server_rust::schema::userfriends::dsl::*;
    use tubepeek_server_rust::schema::usermaster::dsl::*;

    match social_identity_maybe {
        Ok(social_identity) => {
            let now = Utc::now().naive_utc();
            let auth_data_email = social_identity.authData.emailAddress.as_str();
            let google_user_id = social_identity.authData.googleUserId.as_str();

            let save_social_identity =
                |user_record_id: i64,
                 auth_data_email: &str,
                 prov: String,
                 auth_data: &AuthData,
                 now: &NaiveDateTime| {
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

            let existing_user_results = usermaster
                .filter(
                    tubepeek_server_rust::schema::usermaster::dsl::email_address
                        .eq(&social_identity.authData.emailAddress),
                )
                .limit(1)
                .load::<Usermaster>(connection)
                .expect("Error loading users");

            if existing_user_results.len() > 0 {
                let existing_social_identity = social_identities
                    .filter(
                        tubepeek_server_rust::schema::social_identities::dsl::user_id
                            .eq(existing_user_results[0].id)
                            .and(
                                tubepeek_server_rust::schema::social_identities::dsl::provider
                                    .eq(&social_identity.provider),
                            ),
                    )
                    .load::<SocialIdentity>(connection)
                    .expect("Error loading user social identity");

                if (existing_social_identity.len() > 0) {
                    diesel::update(
                        social_identities.filter(
                            tubepeek_server_rust::schema::social_identities::dsl::id
                                .eq(existing_social_identity[0].id),
                        ),
                    )
                    .set((
                        tubepeek_server_rust::schema::social_identities::dsl::full_name
                            .eq(&social_identity.authData.fullName),
                        tubepeek_server_rust::schema::social_identities::dsl::image_url
                            .eq(&social_identity.authData.imageUrl),
                        tubepeek_server_rust::schema::social_identities::dsl::updated_at.eq(&now),
                    ))
                    .execute(connection);
                } else {
                    save_social_identity(
                        existing_user_results[0].id,
                        auth_data_email,
                        social_identity.provider,
                        &social_identity.authData,
                        &now,
                    );
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

                save_social_identity(
                    new_user_db_record.id,
                    auth_data_email,
                    social_identity.provider,
                    &social_identity.authData,
                    &now,
                );
            }

            //--
            let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();

            let mut is_connected_already = false;
            for (conn_id, meta) in connected_clients.iter() {
                if(meta.googleUserId == google_user_id) {
                    is_connected_already = true;
                }
            }

            if is_connected_already == false {
                let mut online_friends : Vec<WsOnlineFriend> = vec![];

                let existing_friends = userfriends
                    .filter(
                        tubepeek_server_rust::schema::userfriends::dsl::user_google_uid
                            .eq(google_user_id),
                    )
                    .load::<tubepeek_server_rust::models::UserFriend>(connection)
                    .expect("Error loading userfriends");

                if (existing_friends.len() > 0) {
                    for friend in existing_friends {
                        for (conn_id, meta) in connected_clients.iter() {
                            if(meta.googleUserId == friend.friend_google_uid) {
                                online_friends.push(WsOnlineFriend {
                                    socketId: meta.socketId,
                                    googleUserId: meta.googleUserId.to_string()
                                });
                            }
                        }
                    }
                }

                connected_clients.insert(
                    ws_client.connection_id(),
                    WsConnectedClientMetadata {
                        socketId: ws_client.connection_id(),
                        socket: ws_client.to_owned(),
                        googleUserId: google_user_id.to_owned(),
                        currentVideo: None,
                        onlineFriends: Box::new(online_friends)
                    },
                );
            }

            println!("connected_clients: {:?}", connected_clients);

            return "all good".to_owned();
        }
        Err(err_msg) => {
            println!("Invalid take social identity.");
            "bad".to_owned()
        }
    };

    "All good".to_owned()
}

fn handle_user_online_status_change(json: &str, connection: &PgConnection) -> String {
    println!("Got UserChangedOnlineStatus message.");

    "All good".to_owned()
}

fn handle_frend_addition(json: &str, connection: &PgConnection) -> String {
    println!("Got AddThisPersonToMyFriendsList message.");

    "All good".to_owned()
}

fn handle_vidoe_change(json: &str, connection: &PgConnection, ws_client: &Sender) -> String {
    println!("Got ChangedVideo message.");
    let video_change_maybe: Result<VideoChangeMessage, Error> = serde_json::from_str(json);

    use tubepeek_server_rust::schema::social_identities::dsl::*;
    use tubepeek_server_rust::schema::usermaster::dsl::*;

    match video_change_maybe {
        Ok(video_change) => {
            let video_url = video_change.videoUrl.as_str();
            let google_user_id = video_change.googleUserId.as_str();
            let youtube_query_url = format!(
                "http://www.youtube.com{}{}", "/oembed?format=json&url=", video_url
            );

            let youtube_response_maybe = reqwest::blocking::get(youtube_query_url.as_str());
            match youtube_response_maybe {
                Ok(valid_response) => {
                    let decoded_video_details =
                        valid_response.json::<YoutubeVideoResponse>().unwrap();
                    let video_title = decoded_video_details.title;
                    let video_thumbnail = decoded_video_details.thumbnail_url;

                    let client_conn_id = ws_client.connection_id();

                    {
                        let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();

                        let conn_metadata_maybe: Option<&mut WsConnectedClientMetadata> =
                            connected_clients.get_mut(&client_conn_id);

                        match conn_metadata_maybe {
                            Some(conn_metadata) => {
                                conn_metadata.currentVideo = Some(WsConnectedClientCurrentVideo {
                                    videoUrl: video_url.to_string(),
                                    title: video_title.to_string(),
                                    thumbnail_url: video_thumbnail.to_string(),
                                });
                            },
                            _ => println!("Don't panic kkkkkkkk"),
                        };
                    }

                    let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();

                    let conn_metadata_maybe: Option<&WsConnectedClientMetadata> =
                        connected_clients.get(&client_conn_id);

                    match conn_metadata_maybe {
                        Some(conn_metadata) => {
                            let broadcast_data = json!({
                                "action": "TakeFriendVideoChange",
                                "googleUserId": google_user_id,
                                "videoData": {
                                    "videoUrl": video_url,
                                    "title": video_title,
                                    "thumbnail_url": video_thumbnail
                                }
                            });

                            for friend in conn_metadata.onlineFriends.iter() {
                                let friend_conn_maybe: Option<&WsConnectedClientMetadata> =
                                    connected_clients.get(&friend.socketId);

                                match friend_conn_maybe {
                                    Some(conn) => {
                                        conn.socket.send(broadcast_data.to_string());
                                    },
                                    None => println!("Done not")
                                };
                            }
                        },
                        _ => println!("Don't panic kkkkkkkk"),
                    };

                    println!("connected_clients: {:?}", connected_clients);
                    println!("Got to this point ...");

                    persist_video_watched(google_user_id, video_url, video_title.as_str(), connection);
                }
                Err(err_msg) => {
                    println!("Invalid video change.");
                }
            };
        }
        Err(err_msg) => {
            println!("Invalid video change.");
        }
    };
    "All good".to_owned()
}

fn persist_video_watched(google_user_id: &str, videoUrl: &str, videoTitle: &str, connection: &PgConnection) {
    use tubepeek_server_rust::schema::social_identities::dsl::*;
    use tubepeek_server_rust::schema::videos::dsl::*;
    use tubepeek_server_rust::schema::uservideos::dsl::*;

    let now = Utc::now().naive_utc();

    let youtube_video_id_maybe: Option<String> = get_youtube_videoid(videoUrl);
    if let None = youtube_video_id_maybe {
        println!("Invalid youtube url metadata");
        return;
    }

    let youtubeVideoId = youtube_video_id_maybe.unwrap();

    let save_user_video =
        |userId: i64,
         videoId: i64,
         now: &NaiveDateTime| {
            let new_user_video = NewUserVideo {
                user_id: userId,
                video_id: videoId,
                created_at: *now,
            };

            diesel::insert_into(uservideos)
                .values(&new_user_video)
                .execute(connection)
                .expect("Error saving new user video");
        };

    let existing_social_identity = social_identities
        .filter(
            tubepeek_server_rust::schema::social_identities::dsl::uid
                .eq(google_user_id)
                .and(
                    tubepeek_server_rust::schema::social_identities::dsl::provider
                        .eq("google")
                ),
        )
        .load::<SocialIdentity>(connection)
        .expect("Error loading user social identity");

    if (existing_social_identity.len() > 0) {
        let existing_video = videos
            .filter(
                tubepeek_server_rust::schema::videos::dsl::youtube_video_id
                    .eq(&youtubeVideoId)
            )
            .load::<Video>(connection)
            .expect("Error loading video");

        if (existing_video.len() == 0) {
            let new_video = NewVideo {
                video_url: videoUrl,
                youtube_video_id: &youtubeVideoId,
                video_title: videoTitle,
                created_at: now,
            };

            let new_video_db_record = diesel::insert_into(videos)
                .values(&new_video)
                .get_result::<Video>(connection)
                .expect("Error saving new video");

            save_user_video(existing_social_identity[0].user_id, new_video_db_record.id, &now);
        } else {
            let existing_user_video = uservideos
                .filter(
                    tubepeek_server_rust::schema::uservideos::dsl::user_id
                        .eq(existing_social_identity[0].user_id)
                        .and(tubepeek_server_rust::schema::uservideos::dsl::video_id
                            .eq(existing_video[0].id))
                )
                .load::<UserVideo>(connection)
                .expect("Error loading user video");

            if (existing_user_video.len() == 0) {
                save_user_video(existing_social_identity[0].user_id, existing_video[0].id, &now);
            }
        }
    }
}

fn main() {
    println!("Tubepeek server up and running ...");

    listen("127.0.0.1:9160", |out| WsServer { out: out }).unwrap()
}
