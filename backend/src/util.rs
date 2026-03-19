use regex::Regex;
use std::sync::LazyLock;

static UNSAFE_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[<>:"/\\|?*]"#).unwrap());

pub fn clean_filename(name: &str) -> String {
    let cleaned = UNSAFE_CHARS.replace_all(name, "");
    cleaned.trim_matches(|c: char| c == '.' || c == ' ').to_string()
}

pub fn match_artist<'a>(name: &str, existing: &'a [String]) -> Option<&'a str> {
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    existing.iter().find_map(|artist| {
        let a = artist.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        if a == normalized { Some(artist.as_str()) } else { None }
    })
}

pub fn parse_duration(dur: &str) -> u64 {
    if let Some((m, s)) = dur.split_once(':') {
        let mins: u64 = m.parse().unwrap_or(0);
        let secs: u64 = s.parse().unwrap_or(0);
        mins * 60 + secs
    } else {
        0
    }
}
