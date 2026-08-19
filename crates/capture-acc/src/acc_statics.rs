use sim_capture_common::SharedMemoryMapping;
use std::collections::HashMap;
use std::sync::LazyLock;

pub const STATIC_NAME: &str = "Local\\acpmf_static";
pub const STATIC_SIZE: usize = 784;

const AC_VERSION_OFFSET: usize = 30;
const CAR_MODEL_OFFSET: usize = 68;
const TRACK_OFFSET: usize = 134;
const PLAYER_NAME_OFFSET: usize = 200;
const PLAYER_SURNAME_OFFSET: usize = 266;
const PLAYER_NICK_OFFSET: usize = 332;

static TRACK_DISPLAY_NAMES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("barcelona", "Barcelona"),
        ("brands_hatch", "Brands Hatch"),
        ("cota", "Circuit of the Americas"),
        ("donington", "Donington Park"),
        ("hungaroring", "Hungaroring"),
        ("imola", "Imola"),
        ("indianapolis", "Indianapolis"),
        ("kyalami", "Kyalami"),
        ("laguna_seca", "Laguna Seca"),
        ("misano", "Misano"),
        ("monza", "Monza"),
        ("mount_panorama", "Mount Panorama"),
        ("nurburgring", "Nurburgring"),
        ("oulton_park", "Oulton Park"),
        ("paul_ricard", "Paul Ricard"),
        ("red_bull_ring", "Red Bull Ring"),
        ("silverstone", "Silverstone"),
        ("snetterton", "Snetterton"),
        ("spa", "Spa-Francorchamps"),
        ("suzuka", "Suzuka"),
        ("watkins_glen", "Watkins Glen"),
        ("zandvoort", "Zandvoort"),
        ("zolder", "Zolder"),
    ])
});

#[derive(Debug, Clone)]
pub struct AccStaticsSnapshot {
    pub ac_version: String,
    pub car_model: String,
    pub track: String,
    pub player_name: String,
    pub player_surname: String,
    pub player_nick: String,
}

impl AccStaticsSnapshot {
    pub fn read(mapping: &SharedMemoryMapping) -> Self {
        Self {
            ac_version: mapping.read_utf16_string_at(AC_VERSION_OFFSET, 15),
            car_model: mapping.read_utf16_string_at(CAR_MODEL_OFFSET, 33),
            track: mapping.read_utf16_string_at(TRACK_OFFSET, 33),
            player_name: mapping.read_utf16_string_at(PLAYER_NAME_OFFSET, 33),
            player_surname: mapping.read_utf16_string_at(PLAYER_SURNAME_OFFSET, 33),
            player_nick: mapping.read_utf16_string_at(PLAYER_NICK_OFFSET, 33),
        }
    }

    pub fn track_name(&self) -> Option<String> {
        resolve_track_name(&self.track)
    }

    pub fn player_display(&self) -> String {
        if !self.player_nick.is_empty() {
            return self.player_nick.clone();
        }
        format!("{} {}", self.player_name, self.player_surname)
            .trim()
            .to_string()
    }
}

pub fn resolve_track_name(track: &str) -> Option<String> {
    let track = track.trim();
    if track.is_empty() {
        return None;
    }

    let normalized = track.to_ascii_lowercase();
    if let Some(display) = TRACK_DISPLAY_NAMES.get(normalized.as_str()) {
        return Some((*display).to_string());
    }

    Some(humanize_track_id(track))
}

fn humanize_track_id(id: &str) -> String {
    id.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(|ch| ch.to_lowercase()))
                    .collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_track_ids() {
        assert_eq!(resolve_track_name("monza"), Some("Monza".to_string()));
        assert_eq!(
            resolve_track_name("spa"),
            Some("Spa-Francorchamps".to_string())
        );
        assert_eq!(
            resolve_track_name("red_bull_ring"),
            Some("Red Bull Ring".to_string())
        );
    }

    #[test]
    fn humanizes_unknown_track_ids() {
        assert_eq!(
            resolve_track_name("some_new_track"),
            Some("Some New Track".to_string())
        );
    }

    #[test]
    fn rejects_empty_track() {
        assert_eq!(resolve_track_name(""), None);
        assert_eq!(resolve_track_name("   "), None);
    }
}
