use std::cmp::Ordering;

/// SemVer precedence without integer truncation; build metadata does not affect order.
pub fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let (left_core, left_pre) = parse(left)?;
    let (right_core, right_pre) = parse(right)?;
    for (left, right) in left_core.iter().zip(right_core.iter()) {
        let order = numeric_cmp(left, right);
        if order != Ordering::Equal {
            return Some(order);
        }
    }
    match (left_pre, right_pre) {
        (None, None) => Some(Ordering::Equal),
        (None, Some(_)) => Some(Ordering::Greater),
        (Some(_), None) => Some(Ordering::Less),
        (Some(left), Some(right)) => {
            let left: Vec<_> = left.split('.').collect();
            let right: Vec<_> = right.split('.').collect();
            for (a, b) in left.iter().zip(right.iter()) {
                let order = match (numeric(a), numeric(b)) {
                    (true, true) => numeric_cmp(a, b),
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => a.cmp(b),
                };
                if order != Ordering::Equal {
                    return Some(order);
                }
            }
            Some(left.len().cmp(&right.len()))
        }
    }
}

pub fn version_valid(version: &str) -> bool {
    parse(version).is_some()
}

fn numeric(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}
fn numeric_cmp(left: &str, right: &str) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn parse(version: &str) -> Option<(Vec<&str>, Option<&str>)> {
    if version.len() > 128 {
        return None;
    }
    let (version, build) = version
        .split_once('+')
        .map_or((version, None), |(a, b)| (a, Some(b)));
    let (core, pre) = version
        .split_once('-')
        .map_or((version, None), |(a, b)| (a, Some(b)));
    let identifiers_valid = |value: &str, prerelease: bool| {
        value.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !(prerelease && numeric(part) && part.len() > 1 && part.starts_with('0'))
        })
    };
    if build.is_some_and(|value| !identifiers_valid(value, false))
        || pre.is_some_and(|value| !identifiers_valid(value, true))
    {
        return None;
    }
    let core: Vec<_> = core.split('.').collect();
    if core.len() != 3
        || core
            .iter()
            .any(|part| !numeric(part) || (part.len() > 1 && part.starts_with('0')))
    {
        return None;
    }
    Some((core, pre))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orders_releases_and_prereleases() {
        let versions = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.10.0",
            "2.0.0",
        ];
        for pair in versions.windows(2) {
            assert_eq!(compare_versions(pair[0], pair[1]), Some(Ordering::Less));
        }
        assert_eq!(
            compare_versions("1.0.0+abc", "1.0.0+def"),
            Some(Ordering::Equal)
        );
    }
    #[test]
    fn rejects_path_suffixes_and_malformed_versions() {
        for version in [
            "1.0.0-../../escape",
            "1.0.0-foo/bar",
            "1.0.0-x\\y",
            "01.0.0",
            "1.0",
            "1.0.0-01",
            "1.0.0+",
            "1.0.0-",
        ] {
            assert!(!version_valid(version), "{version}");
        }
    }
}
