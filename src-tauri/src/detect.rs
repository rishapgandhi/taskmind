//! Meeting detection from window titles (v1 keywords, editable). Pure logic, unit tested.
//! A match must hold for 2 samples before acting and be gone for 3 before auto-stop,
//! so a quick tab switch does not split a meeting.

pub const DEFAULT_KEYWORDS: &[&str] = &[
    "Meet - ",
    "meet.google.com",
    "Zoom Meeting",
    "Zoom Webinar",
    "Meeting in ",
    "Huddle",
    "Webex Meeting",
];

const START_AFTER: u32 = 2;
const STOP_AFTER: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Ask,
    Auto,
}

impl Mode {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("off") => Mode::Off,
            Some("auto") => Mode::Auto,
            _ => Mode::Ask,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// Ask the user (Ask mode) or start recording (Auto mode). Carries the window title.
    Detected { title: String },
    /// The meeting window closed. Stop if we started the recording automatically.
    Ended,
}

#[derive(Debug, Default)]
pub struct Detector {
    seen: u32,
    missing: u32,
    active: bool,
}

impl Detector {
    /// Feed one sample: the meeting window title if any keyword matched.
    pub fn update(&mut self, matched: Option<String>) -> Option<Event> {
        match matched {
            Some(title) => {
                self.missing = 0;
                self.seen += 1;
                if !self.active && self.seen >= START_AFTER {
                    self.active = true;
                    return Some(Event::Detected { title });
                }
                None
            }
            None => {
                self.seen = 0;
                if self.active {
                    self.missing += 1;
                    if self.missing >= STOP_AFTER {
                        self.active = false;
                        self.missing = 0;
                        return Some(Event::Ended);
                    }
                }
                None
            }
        }
    }
}

/// First window title containing any keyword (case-insensitive).
pub fn find_match<'a>(
    titles: impl IntoIterator<Item = &'a str>,
    keywords: &[String],
) -> Option<String> {
    let keys: Vec<String> = keywords
        .iter()
        .map(|k| k.to_lowercase())
        .filter(|k| !k.trim().is_empty())
        .collect();
    titles
        .into_iter()
        .find(|t| {
            let t = t.to_lowercase();
            keys.iter().any(|k| t.contains(k))
        })
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_two_samples_and_three_misses() {
        let mut d = Detector::default();
        let m = || Some("Meet - abc-defg-hij".to_string());
        assert_eq!(d.update(m()), None);
        assert_eq!(
            d.update(m()),
            Some(Event::Detected {
                title: "Meet - abc-defg-hij".into()
            })
        );
        assert_eq!(d.update(m()), None); // no repeat prompt
        assert_eq!(d.update(None), None);
        assert_eq!(d.update(m()), None); // brief tab switch
        assert_eq!(d.update(None), None);
        assert_eq!(d.update(None), None);
        assert_eq!(d.update(None), Some(Event::Ended));
        assert_eq!(d.update(None), None);
    }

    #[test]
    fn single_blip_is_ignored() {
        let mut d = Detector::default();
        assert_eq!(d.update(Some("Zoom Meeting".into())), None);
        assert_eq!(d.update(None), None);
        assert_eq!(d.update(Some("Zoom Meeting".into())), None);
    }

    #[test]
    fn keyword_matching() {
        let keys: Vec<String> = DEFAULT_KEYWORDS.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            find_match(["Inbox", "Meet - xyz - Google Chrome"], &keys).as_deref(),
            Some("Meet - xyz - Google Chrome")
        );
        assert_eq!(
            find_match(["zoom meeting"], &keys).as_deref(),
            Some("zoom meeting")
        );
        assert_eq!(find_match(["Microsoft Teams"], &keys), None);
        assert_eq!(find_match(["x"], &["  ".to_string()]), None);
        assert_eq!(Mode::parse(Some("auto")), Mode::Auto);
        assert_eq!(Mode::parse(None), Mode::Ask);
    }
}
