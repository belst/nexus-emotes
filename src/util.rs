use nexus::localization::translate;

pub fn e(s: &str) -> String {
    translate(s).unwrap_or_else(|| s.to_string())
}
