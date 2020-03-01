-- Your SQL goes here

create table usermaster (
  id bigserial primary key not null,
  uid text not null,
  provider text not null,
  full_name text not null,
  image_url text not null,
  created_at timestamp not null,
  updated_at timestamp
);

create table userfriends (
  id bigserial primary key not null,
  user_google_uid text not null,
  friend_google_uid text not null,
  is_friend_excluded boolean not null,
  created_at timestamp not null,
  updated_at timestamp
);

create table videos (
  id bigserial primary key not null,
  video_url text not null,
  youtube_video_id text not null,
  video_title text not null,
  created_at timestamp not null,
  updated_at timestamp
);

create table uservideos (
  id bigserial primary key not null,
  user_id BIGINT not null REFERENCES usermaster(id),
  video_id BIGINT not null REFERENCES videos(id),
  created_at timestamp not null,
  updated_at timestamp
);
