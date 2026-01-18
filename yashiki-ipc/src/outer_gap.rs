use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OuterGap {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl OuterGap {
    pub fn all(value: u32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn vertical_horizontal(vertical: u32, horizontal: u32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub fn from_args(args: &[String]) -> Option<Self> {
        match args.len() {
            1 => args[0].parse().ok().map(Self::all),
            2 => {
                let v = args[0].parse().ok()?;
                let h = args[1].parse().ok()?;
                Some(Self::vertical_horizontal(v, h))
            }
            4 => {
                let top = args[0].parse().ok()?;
                let right = args[1].parse().ok()?;
                let bottom = args[2].parse().ok()?;
                let left = args[3].parse().ok()?;
                Some(Self {
                    top,
                    right,
                    bottom,
                    left,
                })
            }
            _ => None,
        }
    }

    pub fn horizontal(&self) -> u32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> u32 {
        self.top + self.bottom
    }
}

impl std::fmt::Display for OuterGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.top, self.right, self.bottom, self.left
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outer_gap_all() {
        let gap = OuterGap::all(10);
        assert_eq!(gap.top, 10);
        assert_eq!(gap.right, 10);
        assert_eq!(gap.bottom, 10);
        assert_eq!(gap.left, 10);
    }

    #[test]
    fn test_outer_gap_vertical_horizontal() {
        let gap = OuterGap::vertical_horizontal(10, 20);
        assert_eq!(gap.top, 10);
        assert_eq!(gap.bottom, 10);
        assert_eq!(gap.right, 20);
        assert_eq!(gap.left, 20);
    }

    #[test]
    fn test_outer_gap_from_args_one_value() {
        let gap = OuterGap::from_args(&["10".to_string()]).unwrap();
        assert_eq!(gap.top, 10);
        assert_eq!(gap.right, 10);
        assert_eq!(gap.bottom, 10);
        assert_eq!(gap.left, 10);
    }

    #[test]
    fn test_outer_gap_from_args_two_values() {
        let gap = OuterGap::from_args(&["10".to_string(), "20".to_string()]).unwrap();
        assert_eq!(gap.top, 10);
        assert_eq!(gap.bottom, 10);
        assert_eq!(gap.right, 20);
        assert_eq!(gap.left, 20);
    }

    #[test]
    fn test_outer_gap_from_args_four_values() {
        let gap = OuterGap::from_args(&[
            "10".to_string(),
            "20".to_string(),
            "30".to_string(),
            "40".to_string(),
        ])
        .unwrap();
        assert_eq!(gap.top, 10);
        assert_eq!(gap.right, 20);
        assert_eq!(gap.bottom, 30);
        assert_eq!(gap.left, 40);
    }

    #[test]
    fn test_outer_gap_from_args_invalid() {
        assert!(OuterGap::from_args(&[]).is_none());
        assert!(
            OuterGap::from_args(&["10".to_string(), "20".to_string(), "30".to_string()]).is_none()
        );
        assert!(OuterGap::from_args(&["abc".to_string()]).is_none());
    }

    #[test]
    fn test_outer_gap_horizontal_vertical() {
        let gap = OuterGap {
            top: 10,
            right: 20,
            bottom: 30,
            left: 40,
        };
        assert_eq!(gap.horizontal(), 60);
        assert_eq!(gap.vertical(), 40);
    }

    #[test]
    fn test_outer_gap_display() {
        let gap = OuterGap {
            top: 10,
            right: 20,
            bottom: 30,
            left: 40,
        };
        assert_eq!(format!("{}", gap), "10 20 30 40");
    }

    #[test]
    fn test_outer_gap_serialization() {
        let gap = OuterGap::all(10);
        let json = serde_json::to_string(&gap).unwrap();
        assert!(json.contains("\"top\":10"));
        assert!(json.contains("\"right\":10"));
        assert!(json.contains("\"bottom\":10"));
        assert!(json.contains("\"left\":10"));

        let deserialized: OuterGap = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, gap);
    }
}
