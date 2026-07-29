use crate::error::{Error, Result};

pub(crate) fn map_into_scratch<T, U, I>(
    scratch: &mut Vec<U>,
    input: I,
    mut map: impl FnMut(T) -> Result<U>,
) -> Result<()>
where
    I: IntoIterator<Item = T>,
    I::IntoIter: ExactSizeIterator,
{
    let input = input.into_iter();
    scratch.clear();
    scratch
        .try_reserve(input.len())
        .map_err(|_| Error::AllocationFailed)?;
    for value in input {
        match map(value) {
            Ok(mapped) => scratch.push(mapped),
            Err(error) => {
                scratch.clear();
                return Err(error);
            }
        }
    }
    Ok(())
}

pub(crate) fn commit_scratch<T>(out: &mut Vec<T>, scratch: &mut Vec<T>) {
    if out.capacity() < scratch.len() {
        std::mem::swap(out, scratch);
        scratch.clear();
    } else {
        out.clear();
        out.append(scratch);
    }
}

pub(crate) fn map_into_scratch_transactional<T, U, I>(
    out: &mut Vec<U>,
    scratch: &mut Vec<U>,
    input: I,
    map: impl FnMut(T) -> Result<U>,
) -> Result<()>
where
    I: IntoIterator<Item = T>,
    I::IntoIter: ExactSizeIterator,
{
    map_into_scratch(scratch, input, map)?;
    commit_scratch(out, scratch);
    Ok(())
}

pub(crate) fn map_into_transactional<T, U, I>(
    out: &mut Vec<U>,
    input: I,
    mut map: impl FnMut(T) -> Result<U>,
) -> Result<()>
where
    I: IntoIterator<Item = T>,
    I::IntoIter: ExactSizeIterator,
{
    let input = input.into_iter();
    let mut mapped = Vec::new();
    mapped
        .try_reserve(input.len())
        .map_err(|_| Error::AllocationFailed)?;
    for value in input {
        mapped.push(map(value)?);
    }
    out.clear();
    out.extend(mapped);
    Ok(())
}

pub(crate) fn replace_from_scratch_transactional<T>(
    out: &mut T,
    scratch: &mut T,
    mut clear: impl FnMut(&mut T),
    fill: impl FnOnce(&mut T) -> Result<()>,
    commit: impl FnOnce(&mut T, &mut T),
) -> Result<()> {
    clear(scratch);
    if let Err(error) = fill(scratch) {
        clear(scratch);
        return Err(error);
    }
    commit(out, scratch);
    clear(scratch);
    Ok(())
}

pub(crate) unsafe fn fill_from_ffi<T>(
    out: &mut Vec<T>,
    capacity: usize,
    fill: impl FnOnce(*mut T, i32) -> i32,
) {
    out.clear();
    if capacity == 0 {
        return;
    }
    if out.capacity() < capacity {
        out.reserve(capacity - out.capacity());
    }
    let cap = i32::try_from(capacity).expect("ffi capacity exceeds i32::MAX");
    let wrote = fill(out.as_mut_ptr(), cap).max(0) as usize;
    unsafe { out.set_len(wrote.min(capacity)) };
}

pub(crate) unsafe fn read_from_ffi<T>(
    capacity: usize,
    fill: impl FnOnce(*mut T, i32) -> i32,
) -> Vec<T> {
    let mut out = Vec::new();
    unsafe { fill_from_ffi(&mut out, capacity, fill) };
    out
}

#[cfg(test)]
mod tests {
    use super::{
        map_into_scratch, map_into_scratch_transactional, replace_from_scratch_transactional,
    };
    use crate::Error;

    #[test]
    fn transactional_mapping_preserves_the_exact_caller_buffer_on_failure() {
        let mut out = Vec::with_capacity(8);
        out.extend([11, 12, 13]);
        let original = out.clone();
        let original_ptr = out.as_ptr();
        let original_capacity = out.capacity();
        let mut scratch = Vec::with_capacity(8);

        let result = map_into_scratch_transactional(&mut out, &mut scratch, [1, 2, 3], |value| {
            if value == 2 {
                Err(Error::NativeFailure)
            } else {
                Ok(value * 10)
            }
        });

        assert_eq!(result, Err(Error::NativeFailure));
        assert_eq!(out, original);
        assert_eq!(out.as_ptr(), original_ptr);
        assert_eq!(out.capacity(), original_capacity);
        assert!(scratch.is_empty());
    }

    #[test]
    fn successful_mapping_preserves_a_sufficient_caller_allocation() {
        let mut out = Vec::with_capacity(8);
        out.extend([11, 12, 13]);
        let old_out_ptr = out.as_ptr();
        let old_out_capacity = out.capacity();
        let mut scratch = Vec::with_capacity(6);
        let old_scratch_ptr = scratch.as_ptr();
        let old_scratch_capacity = scratch.capacity();

        map_into_scratch_transactional(&mut out, &mut scratch, [1, 2, 3], |value| Ok(value * 10))
            .unwrap();

        assert_eq!(out, [10, 20, 30]);
        assert_eq!(out.as_ptr(), old_out_ptr);
        assert_eq!(out.capacity(), old_out_capacity);
        assert!(scratch.is_empty());
        assert_eq!(scratch.as_ptr(), old_scratch_ptr);
        assert_eq!(scratch.capacity(), old_scratch_capacity);
    }

    #[test]
    fn successful_mapping_adopts_a_larger_scratch_allocation() {
        let mut out = vec![11];
        let old_out_capacity = out.capacity();
        let mut scratch = Vec::with_capacity(8);
        let old_scratch_ptr = scratch.as_ptr();
        let old_scratch_capacity = scratch.capacity();

        map_into_scratch_transactional(&mut out, &mut scratch, [1, 2, 3], |value| Ok(value * 10))
            .unwrap();

        assert_eq!(out, [10, 20, 30]);
        assert_eq!(out.as_ptr(), old_scratch_ptr);
        assert_eq!(out.capacity(), old_scratch_capacity);
        assert!(scratch.is_empty());
        assert_eq!(scratch.capacity(), old_out_capacity);
    }

    #[test]
    fn grouped_transaction_commits_only_after_every_buffer_is_filled() {
        #[derive(Debug, PartialEq)]
        struct Buffers {
            first: Vec<i32>,
            second: Vec<i32>,
        }

        impl Buffers {
            fn clear(&mut self) {
                self.first.clear();
                self.second.clear();
            }

            fn commit(out: &mut Self, scratch: &mut Self) {
                super::commit_scratch(&mut out.first, &mut scratch.first);
                super::commit_scratch(&mut out.second, &mut scratch.second);
            }
        }

        let mut out = Buffers {
            first: Vec::with_capacity(8),
            second: Vec::with_capacity(6),
        };
        out.first.extend([11, 12]);
        out.second.extend([21, 22, 23]);
        let original_first = out.first.clone();
        let original_second = out.second.clone();
        let original_first_ptr = out.first.as_ptr();
        let original_second_ptr = out.second.as_ptr();
        let original_first_capacity = out.first.capacity();
        let original_second_capacity = out.second.capacity();
        let mut scratch = Buffers {
            first: Vec::with_capacity(8),
            second: Vec::with_capacity(8),
        };

        let result = replace_from_scratch_transactional(
            &mut out,
            &mut scratch,
            Buffers::clear,
            |scratch| {
                map_into_scratch(&mut scratch.first, [1, 2], Ok)?;
                map_into_scratch(&mut scratch.second, [3, 4], |value| {
                    if value == 4 {
                        Err(Error::NativeFailure)
                    } else {
                        Ok(value)
                    }
                })
            },
            Buffers::commit,
        );

        assert_eq!(result, Err(Error::NativeFailure));
        assert_eq!(out.first, original_first);
        assert_eq!(out.second, original_second);
        assert_eq!(out.first.as_ptr(), original_first_ptr);
        assert_eq!(out.second.as_ptr(), original_second_ptr);
        assert_eq!(out.first.capacity(), original_first_capacity);
        assert_eq!(out.second.capacity(), original_second_capacity);
        assert!(scratch.first.is_empty());
        assert!(scratch.second.is_empty());
    }
}
