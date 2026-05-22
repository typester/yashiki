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

    /// Returns an iterator over tag numbers (1..=32) of the set bits, in ascending order.
    pub fn iter_bits(self) -> impl Iterator<Item = u8> {
        let mask = self.0;
        (1u8..=32).filter(move |n| mask & (1 << (n - 1)) != 0)
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
    fn test_intersects() {
        let tag1 = Tag::new(1);
        let tag2 = Tag::new(2);
        let tag12 = tag1.toggle(tag2); // tags 1 and 2

        assert!(tag1.intersects(tag12));
        assert!(tag2.intersects(tag12));
        assert!(tag12.intersects(tag1));
        assert!(!tag1.intersects(tag2));

        let tag34 = Tag::new(3).toggle(Tag::new(4));
        assert!(!tag34.intersects(tag12));
    }

    #[test]
    fn test_toggle() {
        let tag1 = Tag::new(1);
        let tag2 = Tag::new(2);

        // Toggle on (combine tags)
        let toggled = tag1.toggle(tag2);
        assert_eq!(toggled.mask(), 0b0011);
        assert!(toggled.intersects(tag1));
        assert!(toggled.intersects(tag2));

        // Toggle off
        let toggled_off = toggled.toggle(tag1);
        assert_eq!(toggled_off.mask(), 0b0010);
    }

    #[test]
    fn test_first_tag() {
        assert_eq!(Tag::new(1).first_tag(), Some(1));
        assert_eq!(Tag::new(5).first_tag(), Some(5));
        // tag2 | tag4 = 0b1010, lowest is tag 2
        assert_eq!(Tag::new(2).toggle(Tag::new(4)).first_tag(), Some(2));
        // tag3 | tag4 = 0b1100, lowest is tag 3
        assert_eq!(Tag::new(3).toggle(Tag::new(4)).first_tag(), Some(3));
    }

    #[test]
    fn test_equality() {
        assert_eq!(Tag::new(1), Tag::new(1));
        assert_ne!(Tag::new(1), Tag::new(2));
    }

    #[test]
    fn test_iter_bits_single() {
        assert_eq!(Tag::new(1).iter_bits().collect::<Vec<_>>(), vec![1]);
        assert_eq!(Tag::new(5).iter_bits().collect::<Vec<_>>(), vec![5]);
        assert_eq!(Tag::new(32).iter_bits().collect::<Vec<_>>(), vec![32]);
    }

    #[test]
    fn test_iter_bits_multi_ascending() {
        // tag1 | tag2 | tag4 = bits 0,1,3 -> tags 1,2,4
        let t = Tag::from_mask(0b1011);
        assert_eq!(t.iter_bits().collect::<Vec<_>>(), vec![1, 2, 4]);
    }

    #[test]
    fn test_iter_bits_empty() {
        let t = Tag::from_mask(0);
        assert_eq!(t.iter_bits().collect::<Vec<_>>(), Vec::<u8>::new());
    }

    #[test]
    fn test_iter_bits_all() {
        let t = Tag::from_mask(u32::MAX);
        assert_eq!(
            t.iter_bits().collect::<Vec<_>>(),
            (1u8..=32).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_from_mask() {
        assert_eq!(Tag::from_mask(0b0001).mask(), 0b0001);
        assert_eq!(Tag::from_mask(0b0010).mask(), 0b0010);
        assert_eq!(Tag::from_mask(0b0011).mask(), 0b0011);
        // from_mask with single tag should equal Tag::new
        assert_eq!(Tag::from_mask(0b0001), Tag::new(1));
        assert_eq!(Tag::from_mask(0b0010), Tag::new(2));
        // from_mask with multiple tags
        assert_eq!(Tag::from_mask(0b0011), Tag::new(1).toggle(Tag::new(2)));
    }
}
