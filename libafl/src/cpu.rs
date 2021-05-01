//! Architecture agnostic processor features

/// Read time counter using [`llvmint::readcyclecounter`]
///
/// This function is a wrapper around [`llvmint`] to make it easier to test various
/// implementations of reading a cycle counter. In this way, an experiment only has to
/// change this implementation rather than every instead of [`cpu::read_time_counter`]
#[cfg(target_arch = "x86_64")]
pub fn read_time_counter() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}
