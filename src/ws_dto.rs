use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize)]
pub struct TakeUserMessage {
    pub messageType: String,
    pub provider: String,
    pub authData: AuthData
}

#[derive(Serialize, Deserialize)]
pub struct AuthData {
    pub googleUserId: String,
    pub fullName: String,
    pub imageUrl: String
}

#[derive(Serialize, Deserialize)]
pub struct OnlineStatusChange {
    pub messageType: String,
    pub googleUserId: String,
    pub onlineState: bool
}

#[derive(Serialize, Deserialize)]
pub struct VideoChangeMessage {
    pub messageType: String,
    pub googleUserId: String,
    pub videoUrl: String
}

#[derive(Serialize, Deserialize)]
pub struct MakeFriendshipMessage {
    pub messageType: String,
    pub googleUserId: String,
    pub theFriendsGoogleUserId: String
}

#[derive(Debug, Deserialize)]
pub struct YoutubeVideoResponse {
    pub title: String,
    pub thumbnail_url: String
}
