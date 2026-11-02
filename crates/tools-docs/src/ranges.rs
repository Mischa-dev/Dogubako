//! Page-range syntax, validated locally before anything reaches qpdf.
//!
//! Grammar (qpdf compatible subset): comma-separated items, each item is a
//! page or `a-b` range (descending allowed), where a page is a number, `z`
//! (last page) or `rN` (N-th from the end). An item may end in `:even` or
//! `:odd` to keep only the even/odd positions of that range.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct RangeError(pub String);

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn atom(s: &str, n: u32) -> Result<u32, RangeError> {
    let s = s.trim();
    let v = if s.eq_ignore_ascii_case("z") {
        n
    } else if let Some(r) = s.strip_prefix(['r', 'R']) {
        let k: u32 = r
            .parse()
            .map_err(|_| RangeError(format!("'{s}' is not a page number")))?;
        if k == 0 || k > n {
            return Err(RangeError(format!("'{s}' is outside the document (1-{n})")));
        }
        n - k + 1
    } else {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(RangeError(format!("'{s}' is not a page number")));
        }
        s.parse()
            .map_err(|_| RangeError(format!("'{s}' is not a page number")))?
    };
    if v == 0 || v > n {
        return Err(RangeError(format!(
            "page {v} is outside the document (1-{n})"
        )));
    }
    Ok(v)
}

/// Expand a range expression into 1-based page numbers, in order.
pub fn parse(spec: &str, n: u32) -> Result<Vec<u32>, RangeError> {
    if n == 0 {
        return Err(RangeError("the document has no pages".into()));
    }
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(RangeError("page range is empty".into()));
    }
    let mut out = Vec::new();
    for item in spec.split(',') {
        let item = item.trim();
        if item.is_empty() {
            return Err(RangeError("empty item in page range".into()));
        }
        let (body, parity) = match item.split_once(':') {
            Some((b, p)) => match p.trim().to_ascii_lowercase().as_str() {
                "even" => (b, Some(0)),
                "odd" => (b, Some(1)),
                other => {
                    return Err(RangeError(format!(
                        "unknown modifier ':{other}' (use :even or :odd)"
                    )))
                }
            },
            None => (item, None),
        };
        let pages: Vec<u32> = match body.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (atom(a, n)?, atom(b, n)?);
                if a <= b {
                    (a..=b).collect()
                } else {
                    (b..=a).rev().collect()
                }
            }
            None => vec![atom(body, n)?],
        };
        match parity {
            // qpdf semantics: positions within the range (1st, 3rd, ... are odd).
            Some(p) => out.extend(
                pages
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| (i + 1) % 2 == p)
                    .map(|(_, v)| *v),
            ),
            None => out.extend(pages),
        }
        if out.len() > 1_000_000 {
            return Err(RangeError("page range is too long".into()));
        }
    }
    if out.is_empty() {
        return Err(RangeError("page range selects no pages".into()));
    }
    Ok(out)
}

/// Compress a page list back into qpdf syntax (`1-3,7,9-5`).
pub fn to_qpdf(pages: &[u32]) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < pages.len() {
        let start = pages[i];
        let mut j = i;
        if j + 1 < pages.len() && pages[j + 1] == start + 1 {
            while j + 1 < pages.len() && pages[j + 1] == pages[j] + 1 {
                j += 1;
            }
        } else if j + 1 < pages.len() && start > 1 && pages[j + 1] == start - 1 {
            while j + 1 < pages.len() && pages[j] > 1 && pages[j + 1] == pages[j] - 1 {
                j += 1;
            }
        }
        if j == i {
            parts.push(start.to_string());
        } else {
            parts.push(format!("{}-{}", start, pages[j]));
        }
        i = j + 1;
    }
    parts.join(",")
}

/// Groups separated by `;` or newlines, e.g. `1-3; 4-6; 7-z`.
pub fn parse_groups(spec: &str, n: u32) -> Result<Vec<Vec<u32>>, RangeError> {
    let groups: Vec<&str> = spec
        .split([';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if groups.is_empty() {
        return Err(RangeError("no ranges given".into()));
    }
    groups.into_iter().map(|g| parse(g, n)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse("1-3,5,8-z", 10).unwrap(), vec![1, 2, 3, 5, 8, 9, 10]);
        assert_eq!(parse("z-1", 3).unwrap(), vec![3, 2, 1]);
        assert_eq!(parse("r1", 4).unwrap(), vec![4]);
        assert_eq!(parse("r2-r1", 4).unwrap(), vec![3, 4]);
        assert_eq!(parse("1-z:odd", 5).unwrap(), vec![1, 3, 5]);
        assert_eq!(parse("1-z:even", 5).unwrap(), vec![2, 4]);
        assert_eq!(parse(" 2 , 1 ", 2).unwrap(), vec![2, 1]);
    }

    #[test]
    fn rejects() {
        for bad in [
            "", "0", "11", "1-11", "a", "1,,2", "1-", "-3", "1:foo", "r0", "r11", "1;rm -rf",
            "--help", "1 2",
        ] {
            assert!(parse(bad, 10).is_err(), "{bad:?} should fail");
        }
        assert!(parse("1", 0).is_err());
    }

    #[test]
    fn compresses() {
        assert_eq!(to_qpdf(&[1, 2, 3, 5, 8, 9, 10]), "1-3,5,8-10");
        assert_eq!(to_qpdf(&[3, 2, 1]), "3-1");
        assert_eq!(to_qpdf(&[1, 1]), "1,1");
        assert_eq!(to_qpdf(&[4]), "4");
        assert_eq!(
            parse_groups("1-2; 3\n4-z", 5).unwrap(),
            vec![vec![1, 2], vec![3], vec![4, 5]]
        );
    }
}
