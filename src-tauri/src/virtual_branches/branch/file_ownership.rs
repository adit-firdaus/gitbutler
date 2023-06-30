use std::{fmt, vec};

use anyhow::{Context, Result};

use super::hunk::Hunk;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FileOwnership {
    pub file_path: String,
    pub hunks: Vec<Hunk>,
}

impl TryFrom<&String> for FileOwnership {
    type Error = anyhow::Error;
    fn try_from(value: &String) -> std::result::Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl TryFrom<&str> for FileOwnership {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        let mut parts = value.split(':');
        let file_path = parts.next().unwrap();
        let ranges = match parts.next() {
            Some(raw_ranges) => raw_ranges
                .split(',')
                .map(Hunk::try_from)
                .collect::<Result<Vec<Hunk>>>(),
            None => Ok(vec![]),
        }
        .context(format!("failed to parse ownership ranges: {}", value))?;

        if ranges.is_empty() {
            Err(anyhow::anyhow!("ownership ranges cannot be empty"))?
        } else {
            Ok(Self {
                file_path: file_path.to_string(),
                hunks: ranges,
            })
        }
    }
}

impl TryFrom<String> for FileOwnership {
    type Error = anyhow::Error;
    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::try_from(&value)
    }
}

impl FileOwnership {
    pub fn is_full(&self) -> bool {
        self.hunks.is_empty()
    }

    // return a copy of self, with another ranges added
    pub fn plus(&self, another: &FileOwnership) -> FileOwnership {
        if !self.file_path.eq(&another.file_path) {
            return self.clone();
        }

        if self.hunks.is_empty() {
            // full ownership + partial ownership = full ownership
            return self.clone();
        }

        if another.hunks.is_empty() {
            // partial ownership + full ownership = full ownership
            return another.clone();
        }

        let mut hunks = self.hunks.clone();
        another
            .hunks
            .iter()
            .filter(|hunk| !self.hunks.contains(hunk))
            .for_each(|hunk| {
                hunks.insert(0, hunk.clone());
            });

        FileOwnership {
            file_path: self.file_path.clone(),
            hunks,
        }
    }

    // returns (taken, remaining)
    // if all of the ranges are removed, return None
    pub fn minus(&self, another: &FileOwnership) -> (Option<FileOwnership>, Option<FileOwnership>) {
        if !self.file_path.eq(&another.file_path) {
            // no changes
            return (None, Some(self.clone()));
        }

        if another.hunks.is_empty() {
            // any ownership - full ownership = empty ownership
            return (Some(self.clone()), None);
        }

        if self.hunks.is_empty() {
            // full ownership - partial ownership = full ownership, since we don't know all the
            // hunks.
            return (None, Some(self.clone()));
        }

        let mut left = self.hunks.clone();
        let mut taken = vec![];
        for range in &another.hunks {
            left = left
                .iter()
                .flat_map(|r: &Hunk| -> Vec<Hunk> {
                    if r.eq(range) {
                        taken.push(r.clone());
                        vec![]
                    } else {
                        vec![r.clone()]
                    }
                })
                .collect();
        }

        (
            if taken.is_empty() {
                None
            } else {
                Some(FileOwnership {
                    file_path: self.file_path.clone(),
                    hunks: taken,
                })
            },
            if left.is_empty() {
                None
            } else {
                Some(FileOwnership {
                    file_path: self.file_path.clone(),
                    hunks: left,
                })
            },
        )
    }
}

impl fmt::Display for FileOwnership {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        if self.hunks.is_empty() {
            write!(f, "{}", self.file_path)
        } else {
            write!(
                f,
                "{}:{}",
                self.file_path,
                self.hunks
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<String>>()
                    .join(",")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ownership() {
        let ownership = FileOwnership::try_from("foo/bar.rs:1-2,4-5").unwrap();
        assert_eq!(
            ownership,
            FileOwnership {
                file_path: "foo/bar.rs".to_string(),
                hunks: vec![(1..=2).into(), (4..=5).into()]
            }
        );
    }

    #[test]
    fn parse_ownership_no_ranges() {
        assert!(FileOwnership::try_from("foo/bar.rs").is_err());
    }

    #[test]
    fn ownership_to_from_string() {
        let ownership = FileOwnership {
            file_path: "foo/bar.rs".to_string(),
            hunks: vec![(1..=2).into(), (4..=5).into()],
        };
        assert_eq!(ownership.to_string(), "foo/bar.rs:1-2,4-5".to_string());
        assert_eq!(
            FileOwnership::try_from(&ownership.to_string()).unwrap(),
            ownership
        );
    }

    #[test]
    fn test_plus() {
        vec![
            ("file.txt:1-10", "another.txt:1-5", "file.txt:1-10"),
            ("file.txt:1-10,3-14", "file.txt:3-14", "file.txt:1-10,3-14"),
            ("file.txt:5-10", "file.txt:1-5", "file.txt:1-5,5-10"),
            ("file.txt:1-10", "file.txt:1-5", "file.txt:1-5,1-10"),
            ("file.txt:1-5,2-2", "file.txt:1-10", "file.txt:1-10,1-5,2-2"),
            (
                "file.txt:1-10",
                "file.txt:8-15,20-25",
                "file.txt:20-25,8-15,1-10",
            ),
            ("file.txt:1-10", "file.txt:1-10", "file.txt:1-10"),
            ("file.txt:1-10,3-15", "file.txt:1-10", "file.txt:1-10,3-15"),
        ]
        .into_iter()
        .map(|(a, b, expected)| {
            (
                FileOwnership::try_from(a).unwrap(),
                FileOwnership::try_from(b).unwrap(),
                FileOwnership::try_from(expected).unwrap(),
            )
        })
        .for_each(|(a, b, expected)| {
            let got = a.plus(&b);
            assert_eq!(
                got, expected,
                "{} plus {}, expected {}, got {}",
                a, b, expected, got
            );
        });
    }

    #[test]
    fn test_minus() {
        vec![
            (
                "file.txt:1-10",
                "another.txt:1-5",
                (None, Some("file.txt:1-10")),
            ),
            (
                "file.txt:1-10",
                "file.txt:1-5",
                (None, Some("file.txt:1-10")),
            ),
            (
                "file.txt:1-10",
                "file.txt:11-15",
                (None, Some("file.txt:1-10")),
            ),
            (
                "file.txt:1-10",
                "file.txt:1-10",
                (Some("file.txt:1-10"), None),
            ),
            (
                "file.txt:1-10,11-15",
                "file.txt:11-15",
                (Some("file.txt:11-15"), Some("file.txt:1-10")),
            ),
            (
                "file.txt:1-10,11-15,15-17",
                "file.txt:1-10,15-17",
                (Some("file.txt:1-10,15-17"), Some("file.txt:11-15")),
            ),
        ]
        .into_iter()
        .map(|(a, b, expected)| {
            (
                FileOwnership::try_from(a).unwrap(),
                FileOwnership::try_from(b).unwrap(),
                (
                    expected.0.map(|s| FileOwnership::try_from(s).unwrap()),
                    expected.1.map(|s| FileOwnership::try_from(s).unwrap()),
                ),
            )
        })
        .for_each(|(a, b, expected)| {
            let got = a.minus(&b);
            assert_eq!(
                got, expected,
                "{} minus {}, expected {:?}, got {:?}",
                a, b, expected, got
            );
        });
    }

    #[test]
    fn test_equal() {
        vec![
            ("file.txt:1-10", "file.txt:1-10", true),
            ("file.txt:1-10", "file.txt:1-11", false),
            ("file.txt:1-10,11-15", "file.txt:11-15,1-10", false),
            ("file.txt:1-10,11-15", "file.txt:1-10,11-15", true),
        ]
        .into_iter()
        .map(|(a, b, expected)| {
            (
                FileOwnership::try_from(a).unwrap(),
                FileOwnership::try_from(b).unwrap(),
                expected,
            )
        })
        .for_each(|(a, b, expected)| {
            assert_eq!(a == b, expected, "{} == {}, expected {}", a, b, expected);
        });
    }
}