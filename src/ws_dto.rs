use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize)]
pub struct TakeUserMessage {
    pub action: String,
    pub provider: String,
    pub authData: AuthData
}

#[derive(Serialize, Deserialize)]
pub struct AuthData {
    pub uid: String,
    pub fullName: String,
    pub imageUrl: String
}

#[derive(Serialize, Deserialize)]
pub struct OnlineStatusChange {
    pub action: String,
    pub googleUserId: String,
    pub onlineState: bool
}

#[derive(Serialize, Deserialize)]
pub struct VideoChangeMessage {
    pub action: String,
    pub googleUserId: String,
    pub videoUrl: String
}

#[derive(Serialize, Deserialize)]
pub struct FriendExclusionMessage {
    pub action: String,
    pub googleUserId: String,
    pub friendGoogleUserId: String,
    pub exclude: bool
}

#[derive(Serialize, Deserialize)]
pub struct MakeFriendshipMessage {
    pub action: String,
    pub googleUserId: String,
    pub friendGoogleUserId: String
}

#[derive(Debug, Deserialize)]
pub struct YoutubeVideoResponse {
    pub title: String,
    pub thumbnail_url: String
}
