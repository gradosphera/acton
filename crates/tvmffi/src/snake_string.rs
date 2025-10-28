use crate::stack::{Tuple, TupleItem, TupleSlice};
use num_bigint::BigInt;
use tonlib_core::cell::{ArcCell, CellBuilder};

pub fn snake_string_from_slice(slice: &TupleSlice) -> Option<String> {
    let TupleSlice {
        cell,
        start_bits,
        end_bits,
        start_refs,
        ..
    } = slice;

    let mut all_bits = Vec::new();

    let mut parser = cell.parser();
    parser.skip_bits(*start_bits as usize).ok()?;
    let bits_to_load = (end_bits - start_bits) as usize;
    if (bits_to_load % 8) != 0 {
        // this is most likely not a snake string
        return None;
    }

    let bytes_to_load = bits_to_load / 8;

    let bits = parser.load_bits(bytes_to_load * 8).ok()?;
    all_bits.extend_from_slice(&bits);

    if bytes_to_load < 127 {
        // no need to look up to refs
        let result = String::from_utf8(all_bits).ok()?;
        return Some(result);
    }

    // skip references if needed
    for _ in 0..*start_refs {
        parser.next_reference().ok()?;
    }

    let mut next_data_ref = parser.next_reference().ok()?;

    loop {
        let mut parser = next_data_ref.parser();

        let bytes_to_load = parser.remaining_bits() / 8;
        let bits = parser.load_bits(bytes_to_load * 8).ok()?;
        all_bits.extend_from_slice(&bits);

        if parser.remaining_refs() == 0 {
            // this cell is the end
            break;
        }

        next_data_ref = parser.next_reference().unwrap()
    }

    let result = String::from_utf8(all_bits).ok()?;
    Some(result)
}

impl Tuple {
    pub fn push_string(&mut self, s: &str) {
        let bytes = s.as_bytes();
        let total_bits = bytes.len() * 8;

        if total_bits <= 1023 {
            let mut b = CellBuilder::new();
            b.store_bits(total_bits, bytes).unwrap();
            self.push(TupleItem::Slice(TupleSlice {
                cell: ArcCell::from(b.build().unwrap()),
                start_bits: 0,
                end_bits: total_bits as u32,
                end_refs: 0,
                start_refs: 0,
            }));
        } else {
            let mut remaining_bytes = bytes;
            let mut cell_data = Vec::new();

            while !remaining_bytes.is_empty() {
                let chunk_size = std::cmp::min(remaining_bytes.len(), 127); // 127 bytes = 1016 bits < 1023
                let chunk = &remaining_bytes[..chunk_size];
                cell_data.push((chunk, chunk.len() * 8));
                remaining_bytes = &remaining_bytes[chunk_size..];
            }

            // build cells from last to first
            let cell_count = cell_data.len();
            let first_cell_bits = cell_data[0].1 as u32;
            let mut next_cell: Option<ArcCell> = None;

            for (chunk, bits) in cell_data.into_iter().rev() {
                let mut b = CellBuilder::new();
                b.store_bits(bits, chunk).unwrap();

                if let Some(next) = next_cell {
                    b.store_reference(&next).unwrap();
                }

                next_cell = Some(ArcCell::from(b.build().unwrap()));
            }

            let root_cell = next_cell.unwrap();
            let refs_count = if cell_count > 1 { 1 } else { 0 };
            self.push(TupleItem::Slice(TupleSlice {
                cell: root_cell,
                start_bits: 0,
                end_bits: first_cell_bits,
                end_refs: refs_count,
                start_refs: 0,
            }));
        }
    }

    pub fn push_bool(&mut self, v: bool) {
        self.push(TupleItem::Int(if v {
            BigInt::from(-1)
        } else {
            BigInt::from(0)
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::{parse_tuple, serialize_tuple};
    use crate::stack::Tuple;

    #[test]
    fn test_string_roundtrip() {
        // Test small string (fits in one cell)
        let small_string = "Hello World";
        let mut tuple = Tuple::empty();
        tuple.push_string(small_string);
        let serialized = serialize_tuple(&tuple).unwrap();
        let deserialized = parse_tuple(&serialized).unwrap();
        assert_eq!(tuple.0, deserialized);

        // Test large string (requires SnakeString)
        let large_string = "A".repeat(200); // 200 bytes = 1600 bits > 1023
        let mut tuple = Tuple::empty();
        tuple.push_string(&large_string);
        let serialized = serialize_tuple(&tuple).unwrap();
        let deserialized = parse_tuple(&serialized).unwrap();
        assert_eq!(tuple.0, deserialized);
    }
}
