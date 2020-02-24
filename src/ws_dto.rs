use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize)]
pub struct OnlineStatusChange {
    pub messageType: String,
    pub googleUserId: String,
    pub onlineState: bool
}

#[derive(Serialize, Deserialize)]
pub struct AuthData {
    pub googleUserId: String,
    pub fullName: String,
    pub emailAddress: String,
    pub accessToken: String,
    pub accessTokenExpiry: String,
    pub imageUrl: String
}

#[derive(Serialize, Deserialize)]
pub struct TakeSocialIdentityMessage {
    pub messageType: String,
    pub provider: String,
    pub authData: AuthData,
    pub friends: Vec<UserFriend>,
}

#[derive(Serialize, Deserialize)]
pub struct UserFriend {
    pub googleUserId: String,
    pub fullName: String,
    pub imageUrl: String,
}

#[derive(Serialize, Deserialize)]
pub struct VideoChangeMessage {
    pub messageType: String,
    pub googleUserId: String,
    pub videoUrl: String
}