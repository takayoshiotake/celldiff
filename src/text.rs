//! Grapheme-based diff. Original text is never normalized.
use similar::{ChangeTag, TextDiff};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub tag: ChangeTag,
    pub value: String,
}

pub fn segments(left: &str, right: &str) -> Vec<Segment> {
    if left == right {
        return if left.is_empty() {
            Vec::new()
        } else {
            vec![Segment {
                tag: ChangeTag::Equal,
                value: left.into(),
            }]
        };
    }
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(100))
        .diff_graphemes(left, right);
    let mut result: Vec<Segment> = Vec::new();
    for change in diff.iter_all_changes() {
        if change.value().is_empty() {
            continue;
        }
        if let Some(last) = result.last_mut()
            && last.tag == change.tag()
        {
            last.value.push_str(change.value());
            continue;
        }
        result.push(Segment {
            tag: change.tag(),
            value: change.value().to_owned(),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check(left: &str, right: &str) {
        let parts = segments(left, right);
        let old: String = parts
            .iter()
            .filter(|p| p.tag != ChangeTag::Insert)
            .map(|p| p.value.as_str())
            .collect();
        let new: String = parts
            .iter()
            .filter(|p| p.tag != ChangeTag::Delete)
            .map(|p| p.value.as_str())
            .collect();
        assert_eq!(old, left);
        assert_eq!(new, right);
        assert!(parts.iter().all(|p| !p.value.is_empty()));
        assert!(parts.windows(2).all(|p| p[0].tag != p[1].tag));
    }
    #[test]
    fn reconstruction_for_insert_delete_replace_and_repetition() {
        for (a, b) in [
            ("", ""),
            ("", "new"),
            ("old", ""),
            ("東京都", "京都府"),
            ("abc", "aXbc"),
            ("aaaaab", "abaaab"),
        ] {
            check(a, b);
        }
    }
    #[test]
    fn does_not_split_combining_marks_or_emoji() {
        for (a, b) in [("e\u{301}", "e\u{300}"), ("👩‍💻", "👨‍💻"), ("🇯🇵", "🇰🇷")]
        {
            check(a, b);
            let parts = segments(a, b);
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0].value, a);
            assert_eq!(parts[1].value, b);
        }
    }
    #[test]
    fn normalization_is_not_implicit() {
        check("é", "e\u{301}");
        assert!(
            segments("é", "e\u{301}")
                .iter()
                .all(|p| p.tag != ChangeTag::Equal)
        );
    }
    #[test]
    fn long_dissimilar_values_remain_lossless() {
        check(&"a".repeat(10000), &"b".repeat(10000));
    }
}
