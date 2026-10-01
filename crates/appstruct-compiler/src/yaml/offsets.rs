//! Character-index to byte-offset conversion for YAML source spans.
//!
//! `saphyr` reports character indexes while diagnostics require byte offsets. One
//! linear pass records only the boundaries where UTF-8 uses more than one byte. ASCII
//! documents need no index entries, and every lookup is logarithmic in the number of
//! multibyte characters regardless of parser traversal order.

/// Character-index to byte-offset converter for one source document.
#[derive(Default)]
pub(crate) struct CharToByte {
    source_len: usize,
    /// `(character boundary, cumulative extra UTF-8 bytes)` in ascending order.
    multibyte_boundaries: Vec<(usize, usize)>,
}

impl CharToByte {
    pub(crate) fn with_source(source: &str) -> Self {
        let mut extra_bytes = 0;
        let multibyte_boundaries = source
            .chars()
            .enumerate()
            .filter_map(|(char_index, character)| {
                let extra = character.len_utf8() - 1;
                if extra == 0 {
                    return None;
                }
                extra_bytes += extra;
                Some((char_index + 1, extra_bytes))
            })
            .collect();
        Self {
            source_len: source.len(),
            multibyte_boundaries,
        }
    }

    pub(crate) fn byte_offset(&self, char_index: usize) -> usize {
        let boundary = self
            .multibyte_boundaries
            .partition_point(|&(character_boundary, _)| character_boundary <= char_index);
        let extra_bytes = boundary
            .checked_sub(1)
            .map_or(0, |index| self.multibyte_boundaries[index].1);
        char_index.saturating_add(extra_bytes).min(self.source_len)
    }
}

#[cfg(test)]
mod tests {
    use super::CharToByte;

    #[test]
    fn converts_ascii_and_multibyte_indexes() {
        let source = "label: 项目\nname: Project\n";
        let offsets = CharToByte::with_source(source);
        assert_eq!(offsets.byte_offset(0), 0);
        assert_eq!(offsets.byte_offset(7), "label: ".len());
        assert_eq!(offsets.byte_offset(8), "label: ".len() + "项".len());
        assert_eq!(offsets.byte_offset(source.chars().count()), source.len());
        assert_eq!(offsets.byte_offset(usize::MAX), source.len());
    }

    #[test]
    fn supports_out_of_order_lookups_without_copying_ascii_boundaries() {
        let source = "a项b目c";
        let offsets = CharToByte::with_source(source);
        assert_eq!(offsets.multibyte_boundaries.len(), 2);
        assert_eq!(offsets.byte_offset(4), "a项b目".len());
        assert_eq!(offsets.byte_offset(1), "a".len());
        assert_eq!(offsets.byte_offset(3), "a项b".len());

        let ascii = CharToByte::with_source("plain ASCII");
        assert!(ascii.multibyte_boundaries.is_empty());
    }
}
