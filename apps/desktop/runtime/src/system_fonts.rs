use std::collections::HashSet;

use zed_font_kit::source::SystemSource;

pub fn terminal_font_families() -> Vec<String> {
    let source = SystemSource::new();
    let Ok(families) = source.all_families() else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for family in families {
        let family = family.trim();
        if family.is_empty() || family.starts_with('.') || !seen.insert(family.to_lowercase()) {
            continue;
        }
        if family_is_monospace(&source, family) {
            names.push(family.to_string());
        }
    }
    names.sort_by_key(|family| family.to_lowercase());
    names
}

fn family_is_monospace(source: &SystemSource, family: &str) -> bool {
    let Ok(handle) = source.select_family_by_name(family) else {
        return false;
    };
    handle
        .fonts()
        .iter()
        .filter_map(|font| font.load().ok())
        .any(|font| font.is_monospace())
}

#[cfg(test)]
mod tests {
    use super::terminal_font_families;

    #[test]
    fn terminal_font_families_are_clean_sorted_and_unique() {
        let families = terminal_font_families();
        let mut sorted = families.clone();
        sorted.sort_by_key(|family| family.to_lowercase());
        sorted.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

        assert_eq!(families, sorted);
        assert!(families.iter().all(|family| !family.trim().is_empty()));
        assert!(families.iter().all(|family| !family.starts_with('.')));
    }
}
