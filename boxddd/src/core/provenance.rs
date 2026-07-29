use crate::error::{Error, Result};
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

trait TokenKind: Copy {
    fn from_nonzero(value: NonZeroU64) -> Self;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OwnerToken(NonZeroU64);

impl OwnerToken {
    #[cfg(test)]
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

impl TokenKind for OwnerToken {
    fn from_nonzero(value: NonZeroU64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ResourceToken(NonZeroU64);

impl ResourceToken {
    #[cfg(test)]
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ContactEpoch(NonZeroU64);

impl ContactEpoch {
    pub(crate) const INITIAL: Self = Self(NonZeroU64::MIN);

    pub(crate) fn next(self) -> Result<Self> {
        self.0
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(Error::ProvenanceExhausted)
    }
}

impl TokenKind for ResourceToken {
    fn from_nonzero(value: NonZeroU64) -> Self {
        Self(value)
    }
}

struct TokenAllocator<T> {
    next: AtomicU64,
    limit: u64,
    marker: PhantomData<fn() -> T>,
}

impl<T: TokenKind> TokenAllocator<T> {
    const fn new() -> Self {
        Self::with_limit(u64::MAX)
    }

    const fn with_limit(limit: u64) -> Self {
        Self {
            next: AtomicU64::new(0),
            limit,
            marker: PhantomData,
        }
    }

    #[cfg(test)]
    const fn with_state(current: u64, limit: u64) -> Self {
        Self {
            next: AtomicU64::new(current),
            limit,
            marker: PhantomData,
        }
    }

    fn allocate(&self) -> Result<T> {
        // Token allocation establishes uniqueness only; owner state is synchronized separately.
        let previous = self
            .next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1).filter(|next| *next <= self.limit)
            })
            .map_err(|_| Error::ProvenanceExhausted)?;
        let value =
            NonZeroU64::new(previous + 1).expect("the provenance allocator never produces zero");
        Ok(T::from_nonzero(value))
    }
}

static OWNER_TOKENS: TokenAllocator<OwnerToken> = TokenAllocator::new();
static RESOURCE_TOKENS: TokenAllocator<ResourceToken> = TokenAllocator::new();

pub(crate) fn allocate_owner_token() -> Result<OwnerToken> {
    OWNER_TOKENS.allocate()
}

pub(crate) fn allocate_resource_token() -> Result<ResourceToken> {
    RESOURCE_TOKENS.allocate()
}

#[cfg(test)]
mod tests {
    use super::{OwnerToken, ResourceToken, TokenAllocator};
    use crate::Error;
    use std::collections::HashSet;
    use std::hash::Hash;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn concurrent_allocations_are_unique_and_nonzero() {
        const THREADS: usize = 4;
        const PER_THREAD: usize = 128;
        let allocator = Arc::new(TokenAllocator::<OwnerToken>::with_limit(
            (THREADS * PER_THREAD) as u64,
        ));
        let barrier = Arc::new(Barrier::new(THREADS));
        let workers = (0..THREADS)
            .map(|_| {
                let allocator = Arc::clone(&allocator);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    (0..PER_THREAD)
                        .map(|_| allocator.allocate().unwrap().get())
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let values = workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        let unique = values.iter().copied().collect::<HashSet<_>>();

        assert_eq!(values.len(), THREADS * PER_THREAD);
        assert_eq!(unique.len(), values.len());
        assert!(values.iter().all(|value| *value != 0));
    }

    #[test]
    fn last_value_is_returned_once_then_exhaustion_is_sticky() {
        let allocator = TokenAllocator::<ResourceToken>::with_limit(2);

        assert_eq!(allocator.allocate().unwrap().get(), 1);
        assert_eq!(allocator.allocate().unwrap().get(), 2);
        assert_eq!(allocator.allocate(), Err(Error::ProvenanceExhausted));
        assert_eq!(allocator.allocate(), Err(Error::ProvenanceExhausted));
    }

    #[test]
    fn maximum_value_is_returned_once_without_wrapping() {
        let allocator = TokenAllocator::<OwnerToken>::with_state(u64::MAX - 1, u64::MAX);

        assert_eq!(allocator.allocate().unwrap().get(), u64::MAX);
        assert_eq!(allocator.allocate(), Err(Error::ProvenanceExhausted));
        assert_eq!(allocator.allocate(), Err(Error::ProvenanceExhausted));
    }

    #[test]
    fn concurrent_last_slot_has_exactly_one_winner() {
        let allocator = Arc::new(TokenAllocator::<OwnerToken>::with_limit(1));
        let barrier = Arc::new(Barrier::new(2));
        let workers = (0..2)
            .map(|_| {
                let allocator = Arc::clone(&allocator);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    allocator.allocate()
                })
            })
            .collect::<Vec<_>>();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Err(Error::ProvenanceExhausted))
                .count(),
            1
        );
    }

    #[test]
    fn discarded_reservation_remains_burned() {
        let allocator = TokenAllocator::<ResourceToken>::with_limit(3);
        let discarded = allocator.allocate().unwrap();
        let published = allocator.allocate().unwrap();

        assert_eq!(discarded.get(), 1);
        assert_eq!(published.get(), 2);
        assert_ne!(discarded, published);
    }

    #[test]
    fn token_types_are_inert_and_thread_safe() {
        fn assert_token<T: Copy + Eq + Hash + Send + Sync + 'static>() {}
        assert_token::<OwnerToken>();
        assert_token::<ResourceToken>();
    }
}
