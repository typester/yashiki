#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag(u32);

impl Tag {
    pub fn new(n: u32) -> Self {
        assert!(n > 0 && n <= 32);
        Self(1 << (n - 1))
    }

    pub fn from_mask(mask: u32) -> Self {
        Self(mask)
    }

    pub fn mask(self) -> u32 {
        self.0
    }

    pub fn intersects(self, other: Tag) -> bool {
        (self.0 & other.0) != 0
    }

    pub fn union(self, other: Tag) -> Self {
        Self(self.0 | other.0)
    }

    pub fn toggle(self, other: Tag) -> Self {
        Self(self.0 ^ other.0)
    }

    /// Returns the tag number (1-32) of the lowest set bit, or None if empty
    pub fn first_tag(self) -> Option<u32> {
        if self.0 == 0 {
            return None;
        }
        Some(self.0.trailing_zeros() + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_correct_bitmask() {
        assert_eq!(Tag::new(1).mask(), 0b0001);
        assert_eq!(Tag::new(2).mask(), 0b0010);
        assert_eq!(Tag::new(3).mask(), 0b0100);
        assert_eq!(Tag::new(4).mask(), 0b1000);
        assert_eq!(Tag::new(32).mask(), 1 << 31);
    }

    #[test]
    #[should_panic]
    fn test_new_panics_on_zero() {
        Tag::new(0);
    }

    #[test]
    #[should_panic]
    fn test_new_panics_on_33() {
        Tag::new(33);
    }

    #[test]
    fn test_from_mask() {
        assert_eq!(Tag::from_mask(0b1010).mask(), 0b1010);
        assert_eq!(Tag::from_mask(0).mask(), 0);
    }

    #[test]
    fn test_intersects() {
        let tag1 = Tag::new(1);
        let tag2 = Tag::new(2);
        let tag12 = Tag::from_mask(0b0011); // tags 1 and 2

        assert!(tag1.intersects(tag12));
        assert!(tag2.intersects(tag12));
        assert!(tag12.intersects(tag1));
        assert!(!tag1.intersects(tag2));
        assert!(!Tag::from_mask(0b1100).intersects(Tag::from_mask(0b0011)));
    }

    #[test]
    fn test_union() {
        let tag1 = Tag::new(1);
        let tag2 = Tag::new(2);
        let union = tag1.union(tag2);

        assert_eq!(union.mask(), 0b0011);
        assert!(union.intersects(tag1));
        assert!(union.intersects(tag2));
    }

    #[test]
    fn test_toggle() {
        let tag1 = Tag::new(1);
        let tag2 = Tag::new(2);

        // Toggle on
        let toggled = tag1.toggle(tag2);
        assert_eq!(toggled.mask(), 0b0011);

        // Toggle off
        let toggled_off = toggled.toggle(tag1);
        assert_eq!(toggled_off.mask(), 0b0010);
    }

    #[test]
    fn test_first_tag() {
        assert_eq!(Tag::new(1).first_tag(), Some(1));
        assert_eq!(Tag::new(5).first_tag(), Some(5));
        assert_eq!(Tag::from_mask(0b1010).first_tag(), Some(2)); // lowest bit is tag 2
        assert_eq!(Tag::from_mask(0b1100).first_tag(), Some(3));
        assert_eq!(Tag::from_mask(0).first_tag(), None);
    }

    #[test]
    fn test_equality() {
        assert_eq!(Tag::new(1), Tag::new(1));
        assert_eq!(Tag::new(1), Tag::from_mask(0b0001));
        assert_ne!(Tag::new(1), Tag::new(2));
    }
}
