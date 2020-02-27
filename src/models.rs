use super::schema::{usermaster, social_identities, userfriends, videos, uservideos};

use chrono::NaiveDateTime;


#[derive(Queryable)]
pub struct Usermaster {
    pub id: i64,
    pub email_address: String,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>
}


#[derive(Queryable)]
pub struct UserFriend {
    pub id: i64,
    pub user_google_uid: String,
    pub friend_google_uid: String,
    pub is_friend_excluded: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>
}


#[derive(Queryable)]
pub struct SocialIdentity {
    pub id: i64,
    pub user_id: i64,
    pub provider: String,
    pub email_address: String,
    pub full_name: String,
    pub uid: String,
    pub image_url: String,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>
}

#[derive(Queryable)]
pub struct Video {
    pub id: i64,
    pub video_url: String,
    pub youtube_video_id: String,
    pub video_title: String,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>
}

#[derive(Queryable)]
pub struct UserVideo {
    pub id: i64,
    pub user_id: i64,
    pub video_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>
}


#[derive(Insertable)]
#[table_name="usermaster"]
pub struct NewUser<'a> {
    pub email_address: &'a str,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[table_name="social_identities"]
pub struct NewSocialIdentity<'a> {
    pub user_id: i64,
    pub provider: String,
    pub email_address: &'a str,
    pub full_name: &'a str,
    pub uid: &'a str,
    pub image_url: &'a str,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[table_name="userfriends"]
pub struct NewUserFriend<'a> {
    pub user_google_uid: &'a str,
    pub friend_google_uid: &'a str,
    pub is_friend_excluded: bool,
    pub created_at: NaiveDateTime
}

#[derive(Insertable)]
#[table_name="videos"]
pub struct NewVideo<'a> {
    pub video_url: &'a str,
    pub youtube_video_id: &'a str,
    pub video_title: &'a str,
    pub created_at: NaiveDateTime,
}

#[derive(Insertable)]
#[table_name="uservideos"]
pub struct NewUserVideo {
    pub user_id: i64,
    pub video_id: i64,
    pub created_at: NaiveDateTime,
}
