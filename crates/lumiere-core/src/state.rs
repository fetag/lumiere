use std::path::PathBuf;
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub last_dir: Option<PathBuf>,
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    /// Browsing preferences. Zero / empty means "not stored yet", and
    /// `show_folders` uses -1 for "not stored yet" because `false` is a value.
    pub thumb_size: u32,
    pub sort: String,
    pub sort_reverse: bool,
    pub filter: String,
    pub show_folders: i32,
    /// Sidebar / Inspector visibility; -1 means "not stored yet".
    pub show_sidebar: i32,
    pub show_inspector: i32,
    /// Whether a slideshow should start in full screen; -1 = "not stored yet".
    pub slideshow_fullscreen: i32,
    /// Folders the user pinned into the sidebar's Locations section.
    pub sidebar_dirs: Vec<PathBuf>,
    /// Recently visited folders, most recent first.
    pub recent_dirs: Vec<PathBuf>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last_dir: None,
            width: 0.0,
            height: 0.0,
            x: 0.0,
            y: 0.0,
            thumb_size: 0,
            sort: String::new(),
            sort_reverse: false,
            filter: String::new(),
            show_folders: -1,
            show_sidebar: -1,
            show_inspector: -1,
            slideshow_fullscreen: -1,
            sidebar_dirs: Vec::new(),
            recent_dirs: Vec::new(),
        }
    }
}

pub fn parse(text: &str) -> State {
    let mut state = State::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = unescape(value);
        match key.trim() {
            "last_dir" => state.last_dir = Some(PathBuf::from(&value)),
            "width" => state.width = value.parse().unwrap_or(0.0),
            "height" => state.height = value.parse().unwrap_or(0.0),
            "x" => state.x = value.parse().unwrap_or(0.0),
            "y" => state.y = value.parse().unwrap_or(0.0),
            "thumb_size" => state.thumb_size = value.parse().unwrap_or(0),
            "sort" => state.sort = value,
            "sort_reverse" => state.sort_reverse = value == "1",
            "filter" => state.filter = value,
            "show_folders" => state.show_folders = value.parse().unwrap_or(-1),
            "show_sidebar" => state.show_sidebar = value.parse().unwrap_or(-1),
            "show_inspector" => state.show_inspector = value.parse().unwrap_or(-1),
            "slideshow_fullscreen" => state.slideshow_fullscreen = value.parse().unwrap_or(-1),
            key if key.starts_with("sidebar_dir_") => {
                if let Ok(i) = key["sidebar_dir_".len()..].parse::<usize>() {
                    if i < 64 {
                        state.sidebar_dirs.push(PathBuf::from(value));
                    }
                }
            }
            key if key.starts_with("recent_dir_") => {
                if let Ok(i) = key["recent_dir_".len()..].parse::<usize>() {
                    if i < 64 {
                        state.recent_dirs.push(PathBuf::from(value));
                    }
                }
            }
            _ => {}
        }
    }
    state.width = valid_size(state.width, 640.0, 16_384.0);
    state.height = valid_size(state.height, 420.0, 16_384.0);
    if !state.x.is_finite() || state.x.abs() > 100_000.0 {
        state.x = 0.0;
    }
    if !state.y.is_finite() || state.y.abs() > 100_000.0 {
        state.y = 0.0;
    }
    if state.thumb_size != 0 {
        state.thumb_size = state.thumb_size.clamp(32, 320);
    }
    state
}
pub fn serialize(state: &State) -> String {
    let mut text = String::new();
    if let Some(dir) = &state.last_dir {
        text.push_str(&format!(
            "last_dir={}\n",
            escape(&dir.display().to_string())
        ));
    }
    if state.width > 0.0 {
        text.push_str(&format!("width={}\n", state.width));
    }
    if state.height > 0.0 {
        text.push_str(&format!("height={}\n", state.height));
    }
    if state.x != 0.0 || state.y != 0.0 {
        text.push_str(&format!("x={}\ny={}\n", state.x, state.y));
    }
    if state.thumb_size > 0 {
        text.push_str(&format!("thumb_size={}\n", state.thumb_size));
    }
    if !state.sort.is_empty() {
        text.push_str(&format!("sort={}\n", escape(&state.sort)));
    }
    text.push_str(&format!("sort_reverse={}\n", state.sort_reverse as i32));
    if !state.filter.is_empty() {
        text.push_str(&format!("filter={}\n", escape(&state.filter)));
    }
    if state.show_folders >= 0 {
        text.push_str(&format!("show_folders={}\n", state.show_folders));
    }
    if state.show_sidebar >= 0 {
        text.push_str(&format!("show_sidebar={}\n", state.show_sidebar));
    }
    if state.show_inspector >= 0 {
        text.push_str(&format!("show_inspector={}\n", state.show_inspector));
    }
    if state.slideshow_fullscreen >= 0 {
        text.push_str(&format!(
            "slideshow_fullscreen={}\n",
            state.slideshow_fullscreen
        ));
    }
    for (i, dir) in state.sidebar_dirs.iter().enumerate() {
        text.push_str(&format!(
            "sidebar_dir_{}={}\n",
            i,
            escape(&dir.display().to_string())
        ));
    }
    for (i, dir) in state.recent_dirs.iter().enumerate() {
        text.push_str(&format!(
            "recent_dir_{}={}\n",
            i,
            escape(&dir.display().to_string())
        ));
    }
    text
}
fn valid_size(value: f32, minimum: f32, maximum: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value.clamp(minimum, maximum)
    } else {
        0.0
    }
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_serializes_to_only_the_sort_flag() {
        assert_eq!(serialize(&State::default()), "sort_reverse=0\n");
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let state = State {
            last_dir: Some("/tmp/pics".into()),
            width: 1280.0,
            height: 800.0,
            x: 10.0,
            y: 20.0,
            thumb_size: 96,
            sort: "name".into(),
            sort_reverse: true,
            filter: "*.png".into(),
            show_folders: 1,
            show_sidebar: 0,
            show_inspector: 1,
            slideshow_fullscreen: 0,
            sidebar_dirs: vec!["/a".into(), "/b".into()],
            recent_dirs: vec!["/c".into()],
        };
        assert_eq!(parse(&serialize(&state)), state);
    }

    #[test]
    fn round_trip_escapes_special_characters() {
        let state = State {
            last_dir: Some("/tmp/a\nb\\c".into()),
            filter: "x=y=z".into(),
            ..State::default()
        };
        assert_eq!(parse(&serialize(&state)), state);
    }

    #[test]
    fn parse_ignores_unknown_keys_and_malformed_lines() {
        let text = "foo=bar\nno-equals-line\nwidth=800\n";
        let state = parse(text);
        assert_eq!(state.width, 800.0);
        assert_eq!(state.sort, String::new());
    }

    #[test]
    fn parse_clamps_window_size() {
        assert_eq!(parse("width=10\n").width, 640.0);
        assert_eq!(parse("height=100000\n").height, 16_384.0);
        assert_eq!(parse("width=abc\n").width, 0.0);
        assert_eq!(parse("width=nan\n").width, 0.0);
    }

    #[test]
    fn parse_resets_out_of_range_positions() {
        assert_eq!(parse("x=200000\n").x, 0.0);
        assert_eq!(parse("y=-200000\n").y, 0.0);
        assert_eq!(parse("x=nan\n").x, 0.0);
        assert_eq!(parse("x=100000\n").x, 100_000.0);
    }

    #[test]
    fn parse_clamps_thumb_size() {
        assert_eq!(parse("thumb_size=10\n").thumb_size, 32);
        assert_eq!(parse("thumb_size=999\n").thumb_size, 320);
        assert_eq!(parse("thumb_size=160\n").thumb_size, 160);
        assert_eq!(parse("thumb_size=0\n").thumb_size, 0);
    }

    #[test]
    fn parse_sort_reverse_is_true_only_for_one() {
        assert!(parse("sort_reverse=1\n").sort_reverse);
        assert!(!parse("sort_reverse=0\n").sort_reverse);
        assert!(!parse("sort_reverse=true\n").sort_reverse);
    }

    #[test]
    fn parse_indexed_dir_entries_with_a_64_cap() {
        let text = "sidebar_dir_0=/a\nsidebar_dir_1=/b\nsidebar_dir_64=/over\
                   \nsidebar_dir_x=/not-a-number\nrecent_dir_0=/c\n";
        let state = parse(text);
        assert_eq!(
            state.sidebar_dirs,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert_eq!(state.recent_dirs, vec![PathBuf::from("/c")]);
    }

    #[test]
    fn parse_trichotomy_flags_fall_back_to_not_stored() {
        assert_eq!(parse("show_folders=1\n").show_folders, 1);
        assert_eq!(parse("show_folders=0\n").show_folders, 0);
        assert_eq!(parse("show_folders=garbage\n").show_folders, -1);
    }
}
