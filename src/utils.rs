use regex::Regex;
use std::string::ToString;


// Got regex from the implementation of https://docs.rs/rafy/0.2.1/rafy/
pub fn get_youtube_videoid(videoUrl: &str) -> Option<String> {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^.*(?:(?:youtu\.be/|v/|vi/|u/w/|embed/)|(?:(?:watch)?\?v(?:i)?=|\&v(?:i)?=))([^#\&\?]*).*").unwrap();
    }

    if RE.is_match(videoUrl) {
        let vid_split = RE.captures(videoUrl).unwrap();
        return Some(vid_split.get(1).unwrap().as_str().to_string());
    }
    None
}
