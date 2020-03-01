extern crate ws;

#[macro_use]
extern crate lazy_static;
extern crate reqwest;
extern crate regex;

extern crate tubepeek_server_rust;

mod db_pool;
use db_pool::{establish_connection, PgPool};

mod ws_dto;
use ws_dto::*;

mod utils;
use utils::*;

use ws::Result as WsResult;
use ws::{listen, CloseCode, Handler, Message, Sender};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::sync::Mutex;

use diesel::prelude::*;
use diesel::PgConnection;
use serde_json::{json, Error, Value as JsonValue};

use chrono::{NaiveDateTime, Utc};
use tubepeek_server_rust::models::{NewUser, NewUserFriend, Usermaster, Video, NewVideo, UserVideo, NewUserVideo, UserFriend};



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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct WsFriendCurrentVideo {
    pub googleUserId: String,
    pub videoData: WsConnectedClientCurrentVideo,
}

struct WsServer {
    out: Sender,
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

        let response = match json_maybe.unwrap()["action"].as_str().unwrap() {
            "TakeUserMessage" => {
                handle_user(&raw_message, &database_connection, &self.out)
            },
            "UserChangedOnlineStatus" => {
                handle_user_online_status_change(&raw_message, &database_connection, &self.out)
            },
            "MakeFriendship" => {
                handle_friendship(&raw_message, &database_connection, &self.out)
            },
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

                println!("connected_clients[ON_DISCONNECT]: {:?}", connected_clients);
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

fn handle_user(json: &str, connection: &PgConnection, ws_client: &Sender) -> String {
    let user_details_maybe: Result<TakeUserMessage, Error> =
        serde_json::from_str(json);

    use tubepeek_server_rust::schema::userfriends::dsl::*;
    use tubepeek_server_rust::schema::usermaster::dsl::*;

    match user_details_maybe {
        Ok(user_details) => {
            let google_user_id = &user_details.authData.googleUserId.to_owned();

            persist_user(user_details, connection);
            //--
            let existing_friends = userfriends
                .filter(
                    tubepeek_server_rust::schema::userfriends::dsl::user_google_uid
                        .eq(google_user_id),
                )
                .load::<tubepeek_server_rust::models::UserFriend>(connection)
                .expect("Error loading userfriends");

            let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();

            let mut is_connected_already = false;
            for (conn_id, meta) in connected_clients.iter() {
                if(meta.googleUserId == *google_user_id) {
                    is_connected_already = true;
                    break;
                }
            }

            let mut online_friends : Vec<WsOnlineFriend> = vec![];
            let mut friends_current_video : Vec<WsFriendCurrentVideo> = vec![];

            for friend in existing_friends {
                for (conn_id, meta) in connected_clients.iter() {
                    if(meta.googleUserId == friend.friend_google_uid) {
                        online_friends.push(WsOnlineFriend {
                            socketId: meta.socketId,
                            googleUserId: meta.googleUserId.to_string()
                        });

                        match &meta.currentVideo {
                            Some(videoDetails) => {
                                friends_current_video.push(WsFriendCurrentVideo {
                                    googleUserId: meta.googleUserId.to_string(),
                                    videoData: WsConnectedClientCurrentVideo {
                                        videoUrl: videoDetails.videoUrl.to_string(),
                                        title: videoDetails.title.to_string(),
                                        thumbnail_url: videoDetails.thumbnail_url.to_string()
                                    }
                                });
                            },
                            None => {}
                        };
                    }
                }
            }

            if is_connected_already == false {
                connected_clients.insert(ws_client.connection_id(),
                    WsConnectedClientMetadata {
                        socketId: ws_client.connection_id(),
                        socket: ws_client.to_owned(),
                        googleUserId: google_user_id.to_owned(),
                        currentVideo: None,
                        onlineFriends: Box::new(online_friends)
                    },
                );
            } else {
                let conn_metadata_maybe: Option<&mut WsConnectedClientMetadata> =
                    connected_clients.get_mut(&ws_client.connection_id());

                match conn_metadata_maybe {
                    Some(conn_metadata) => {
                        conn_metadata.onlineFriends = Box::new(online_friends);
                    },
                    _ => println!("Don't panic kkkkkkkk"),
                };
            }

            println!("connected_clients: {:?}", connected_clients);

            let dataToReplyWith = json!({
                "action": "TakeVideosBeingWatched",
                "friendsOnYoutubeNow": friends_current_video,
                "friendsOnTubePeek": []
            });

            return dataToReplyWith.to_string();
        },
        Err(err_msg) => {
            println!("Invalid take social identity.");
            "bad".to_owned()
        }
    };

    "All good".to_owned()
}


fn persist_user(user_details: TakeUserMessage, connection: &PgConnection) {
    use tubepeek_server_rust::schema::usermaster::dsl::*;

    let now = Utc::now().naive_utc();
    let google_user_id = user_details.authData.googleUserId.as_str();

    let existing_user = usermaster
        .filter(
            tubepeek_server_rust::schema::usermaster::dsl::uid
                .eq(google_user_id),
        )
        .limit(1)
        .load::<Usermaster>(connection)
        .expect("Error loading users");

    if existing_user.len() > 0 {
        diesel::update(
            usermaster.filter(
                tubepeek_server_rust::schema::usermaster::dsl::uid
                    .eq(google_user_id),
            ),
        )
        .set((
            tubepeek_server_rust::schema::usermaster::dsl::full_name
                .eq(&user_details.authData.fullName),
            tubepeek_server_rust::schema::usermaster::dsl::image_url
                .eq(&user_details.authData.imageUrl),
            tubepeek_server_rust::schema::usermaster::dsl::updated_at.eq(&now),
        ))
        .execute(connection);
    } else {
        let new_user = NewUser {
            uid: google_user_id,
            provider: user_details.provider,
            full_name: user_details.authData.fullName.as_str(),
            image_url: user_details.authData.imageUrl.as_str(),
            created_at: now,
        };

        let new_user_db_record = diesel::insert_into(usermaster)
            .values(&new_user)
            .get_result::<Usermaster>(connection)
            .expect("Error saving new user");
    }
}


fn handle_user_online_status_change(json: &str, connection: &PgConnection, ws_client: &Sender) -> String {
    println!("Got UserChangedOnlineStatus message.");

    let online_status_maybe: Result<OnlineStatusChange, Error> =
        serde_json::from_str(json);

    match online_status_maybe {
        Ok(online_status) => {
            let google_user_id = &online_status.googleUserId.to_owned();
            let online_status = &online_status.onlineState;

            let broadcast_data = json!({
                "action": "TakeFriendOnlineStatus",
                "googleUserId": *google_user_id,
                "onlineState": online_status
            });

            let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();

            let conn_metadata_maybe: Option<&WsConnectedClientMetadata> =
                connected_clients.get(&ws_client.connection_id());

            match conn_metadata_maybe {
                Some(conn_metadata) => {
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

            if(!online_status) {
                connected_clients.remove(&ws_client.connection_id());
            }
        },
        Err(err_msg) => {
            println!("Invalid take online status.");
        }
    };

    "All good".to_owned()
}

fn handle_friendship(json: &str, connection: &PgConnection, ws_client: &Sender) -> String {
    println!("Got MakeFriendship message.");

    use tubepeek_server_rust::schema::usermaster::dsl::*;
    use tubepeek_server_rust::schema::userfriends::dsl::*;

    let make_friendship_maybe: Result<MakeFriendshipMessage, Error> =
        serde_json::from_str(json);

    match make_friendship_maybe {
        Ok(make_friendship) => {
            let google_user_id = &make_friendship.googleUserId.to_owned();
            let friend_google_user_id = &make_friendship.googleUserId.to_owned();
            let now = Utc::now().naive_utc();

            let does_friend_exist = userfriends
                .filter(
                    tubepeek_server_rust::schema::userfriends::dsl::user_google_uid
                        .eq(&google_user_id)
                        .and(tubepeek_server_rust::schema::userfriends::dsl::friend_google_uid
                            .eq(&friend_google_user_id)),
                )
                .limit(1)
                .load::<UserFriend>(connection)
                .expect("Error loading user friend");

            if(does_friend_exist.len() == 0) {
                let new_friend = NewUserFriend {
                    user_google_uid: google_user_id,
                    friend_google_uid: friend_google_user_id,
                    is_friend_excluded: false,
                    created_at: now,
                };

                let new_social_id_db_record = diesel::insert_into(userfriends)
                    .values(&new_friend)
                    .execute(connection)
                    .expect("Error saving new friend");
            }
            //--
            let does_reverse_friend_exist = userfriends
                .filter(
                    tubepeek_server_rust::schema::userfriends::dsl::user_google_uid
                        .eq(&friend_google_user_id)
                        .and(tubepeek_server_rust::schema::userfriends::dsl::friend_google_uid
                            .eq(&google_user_id)),
                )
                .limit(1)
                .load::<UserFriend>(connection)
                .expect("Error loading user reverse friend");

            if(does_reverse_friend_exist.len() == 0) {
                let reverse_new_friend = NewUserFriend {
                    user_google_uid: friend_google_user_id,
                    friend_google_uid: google_user_id,
                    is_friend_excluded: false,
                    created_at: now,
                };

                diesel::insert_into(userfriends)
                    .values(&reverse_new_friend)
                    .execute(connection)
                    .expect("Error saving new reverse friend");
            }
            //--
            let current_user = usermaster
                .filter(
                    tubepeek_server_rust::schema::usermaster::dsl::uid
                        .eq(google_user_id),
                )
                .load::<Usermaster>(connection)
                .expect("Error loading current user");

            let friend_user = usermaster
                .filter(
                    tubepeek_server_rust::schema::usermaster::dsl::uid
                        .eq(friend_google_user_id),
                )
                .load::<Usermaster>(connection)
                .expect("Error loading friend user");

            if (current_user.len() > 0 || friend_user.len() > 0) {
                let mut connected_clients = WS_CONNECTED_CLIENTS.lock().unwrap();

                for (conn_id, meta) in connected_clients.iter() {
                    if(current_user.len() > 0 && meta.googleUserId == *friend_google_user_id) {
                        let broadcast_data = json!({
                            "action": "NewFriendInstalledTubePeek",
                            "friendDetails": {
                                "googleUserId": *google_user_id,
                                "fullName": current_user[0].full_name,
                                "imageUrl": current_user[0].image_url
                            }
                        });

                        meta.socket.send(broadcast_data.to_string());
                    }
                    if(friend_user.len() > 0 && meta.googleUserId == *google_user_id) {
                        let broadcast_data = json!({
                            "action": "NewFriendInstalledTubePeek",
                            "friendDetails": {
                                "googleUserId": *friend_google_user_id,
                                "fullName": friend_user[0].full_name,
                                "imageUrl": friend_user[0].image_url
                            }
                        });

                        meta.socket.send(broadcast_data.to_string());
                    }
                }
            }
        },
        Err(err_msg) => {
            println!("Invalid make friendship change.");
        }
    };

    "All good".to_owned()
}

fn handle_vidoe_change(json: &str, connection: &PgConnection, ws_client: &Sender) -> String {
    println!("Got ChangedVideo message.");
    let video_change_maybe: Result<VideoChangeMessage, Error> = serde_json::from_str(json);

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
                        _ => println!("Don't panic!"),
                    };

                    println!("connected_clients: {:?}", connected_clients);
                    println!("Got to this point ...");

                    persist_video_watched(google_user_id, video_url, video_title.as_str(), connection);
                },
                Err(err_msg) => {
                    println!("Invalid youtube response. {:?}", err_msg);
                }
            };
        },
        Err(err_msg) => {
            println!("Invalid video change.");
        }
    };
    "All good".to_owned()
}

fn persist_video_watched(google_user_id: &str, videoUrl: &str, videoTitle: &str, connection: &PgConnection) {
    use tubepeek_server_rust::schema::usermaster::dsl::*;
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

    let existing_user = usermaster
        .filter(
            tubepeek_server_rust::schema::usermaster::dsl::uid
                .eq(google_user_id),
        )
        .load::<Usermaster>(connection)
        .expect("Error loading user");

    if (existing_user.len() > 0) {
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

            save_user_video(existing_user[0].id, new_video_db_record.id, &now);
        } else {
            let existing_user_video = uservideos
                .filter(
                    tubepeek_server_rust::schema::uservideos::dsl::user_id
                        .eq(existing_user[0].id)
                        .and(tubepeek_server_rust::schema::uservideos::dsl::video_id
                            .eq(existing_video[0].id))
                )
                .load::<UserVideo>(connection)
                .expect("Error loading user video");

            if (existing_user_video.len() == 0) {
                save_user_video(existing_user[0].id, existing_video[0].id, &now);
            }
        }
    }
}

fn main() {
    println!("Tubepeek server up and running ...");

    //let server_ip = "192.168.88.205:9160";
    let server_ip = "127.0.0.1:9160";

    listen(server_ip, |out| WsServer { out: out }).unwrap()
}
