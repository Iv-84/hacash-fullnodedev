


#[macro_export]
macro_rules! impl_field_only_new {
    ($class:ident) => {
        impl Field for $class {
            fn new() -> Self {
                Self::default()
            }
        }
    };
}


/// Safe initial capacity for parsing a length-prefixed collection from an
/// untrusted buffer. Caps `count` at `remaining_bytes` to stop a forged length
/// prefix from triggering an enormous allocation.
/// * `count` — element count read from the wire
/// * `remaining_bytes` — bytes left to parse
#[inline]
pub fn prealloc_cap(count: usize, remaining_bytes: usize) -> usize {
    count.min(remaining_bytes)
}

#[cfg(test)]
mod prealloc_cap_tests {
    use super::prealloc_cap;

    #[test]
    fn caps_oversized_count_to_remaining_bytes() {
        // Fake count with a tiny body must not preallocate by `count`
        assert_eq!(prealloc_cap(u32::MAX as usize, 8), 8);
        assert_eq!(prealloc_cap(usize::MAX, 0), 0);
    }

    #[test]
    fn keeps_count_for_legitimate_input() {
        // A valid count (<= remaining bytes) is returned unchanged.
        assert_eq!(prealloc_cap(3, 90), 3);
        assert_eq!(prealloc_cap(10, 10), 10);
    }
}



