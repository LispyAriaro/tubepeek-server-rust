table! {
    userfriends (id) {
        id -> Int8,
        user_google_uid -> Text,
        friend_google_uid -> Text,
        is_friend_excluded -> Bool,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
    }
}

table! {
    usermaster (id) {
        id -> Int8,
        uid -> Text,
        provider -> Text,
        full_name -> Text,
        image_url -> Text,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
    }
}

table! {
    uservideos (id) {
        id -> Int8,
        user_id -> Int8,
        video_id -> Int8,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
    }
}

table! {
    videos (id) {
        id -> Int8,
        video_url -> Text,
        youtube_video_id -> Text,
        video_title -> Text,
        created_at -> Timestamp,
        updated_at -> Nullable<Timestamp>,
    }
}

joinable!(uservideos -> usermaster (user_id));
joinable!(uservideos -> videos (video_id));

allow_tables_to_appear_in_same_query!(
    userfriends,
    usermaster,
    uservideos,
    videos,
);
