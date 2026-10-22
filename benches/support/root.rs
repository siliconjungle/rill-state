//! Alternative snapshot ownership policies over the same backend and journal.
use std::sync::Arc;

pub trait MapRoot<M>: Clone + Send + Sync {
    fn from_map(map: M) -> Self;
    fn as_map(&self) -> &M;
}
impl<M: Send + Sync> MapRoot<M> for Arc<M> {
    #[inline]
    fn from_map(map: M) -> Self {
        Arc::new(map)
    }
    #[inline]
    fn as_map(&self) -> &M {
        self.as_ref()
    }
}
pub struct NativeRoot<M>(M);
impl<M: Clone> Clone for NativeRoot<M> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<M: Clone + Send + Sync> MapRoot<M> for NativeRoot<M> {
    #[inline]
    fn from_map(map: M) -> Self {
        Self(map)
    }
    #[inline]
    fn as_map(&self) -> &M {
        &self.0
    }
}
