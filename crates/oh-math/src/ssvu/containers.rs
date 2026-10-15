//! Port of `extlibs/SSVUtils/include/SSVUtils/Core/Utils/Containers.hpp` —
//! only the helpers the game actually uses (issue 001 task spec §3):
//! `contains`, `getByModIdx`. (C++ `eraseRemoveIf` and the iterator
//! wrappers are not used by the game — see the issue report.)

use crate::ssvu::math::get_mod;

/// `ssvu::contains` — `find(mContainer, mValue) != std::end(mContainer)`
/// (linear search).
#[must_use]
pub fn contains<T: PartialEq>(m_container: &[T], m_value: &T) -> bool {
    m_container.contains(m_value)
}

/// Integral index types accepted by [`get_by_mod_idx`], mirroring the C++
/// template's implicit conversion of `mIdx` to the container's `size_type`
/// (`std::size_t`, two's-complement wrap for negative values).
pub trait AsContainerIdx: Copy {
    /// `static_cast<std::size_t>(mIdx)`.
    fn as_container_idx(self) -> usize;
}

macro_rules! impl_as_container_idx {
    ($($t:ty),* $(,)?) => {
        $(
            impl AsContainerIdx for $t {
                #[inline]
                fn as_container_idx(self) -> usize {
                    self as usize
                }
            }
        )*
    };
}

impl_as_container_idx!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

/// `ssvu::getByModIdx` — `mContainer[getMod(mIdx, mContainer.size())]`.
///
/// C++ has a non-const overload (`operator[]`) and a const overload
/// (`.at()`); both compute the same index and `getMod` guarantees
/// `0 <= idx < size`, so the single slice form covers both.
#[must_use]
pub fn get_by_mod_idx<T, I: AsContainerIdx>(m_container: &[T], m_idx: I) -> &T {
    let idx = get_mod(m_idx.as_container_idx(), m_container.len());
    &m_container[idx]
}
