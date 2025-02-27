//! Observers give insights about runs of a target, such as coverage, timing, stack depth, and more.

pub mod map;
pub use map::*;

pub mod cmp;
pub use cmp::*;

use alloc::string::{String, ToString};
use core::time::Duration;
use serde::{Deserialize, Serialize};

use crate::{
    bolts::{
        current_time,
        tuples::{MatchName, Named},
    },
    executors::HasExecHooks,
    Error,
};

/// Observers observe different information about the target.
/// They can then be used by various sorts of feedback.
pub trait Observer: Named + serde::Serialize + serde::de::DeserializeOwned {
    /// The testcase finished execution, calculate any changes.
    /// Reserved for future use.
    #[inline]
    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

/// A haskell-style tuple of observers
pub trait ObserversTuple: MatchName + serde::Serialize + serde::de::DeserializeOwned {}

impl ObserversTuple for () {}

impl<Head, Tail> ObserversTuple for (Head, Tail)
where
    Head: Observer,
    Tail: ObserversTuple,
{
}

/// A simple observer, just overlooking the runtime of the target.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TimeObserver {
    name: String,
    start_time: Duration,
    last_runtime: Option<Duration>,
}

impl TimeObserver {
    /// Creates a new [`TimeObserver`] with the given name.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name: name.to_string(),
            start_time: Duration::from_secs(0),
            last_runtime: None,
        }
    }

    /// Gets the runtime for the last execution of this target.
    #[must_use]
    pub fn last_runtime(&self) -> &Option<Duration> {
        &self.last_runtime
    }
}

impl Observer for TimeObserver {}

impl<EM, I, S, Z> HasExecHooks<EM, I, S, Z> for TimeObserver {
    fn pre_exec(
        &mut self,
        _fuzzer: &mut Z,
        _state: &mut S,
        _mgr: &mut EM,
        _input: &I,
    ) -> Result<(), Error> {
        self.last_runtime = None;
        self.start_time = current_time();
        Ok(())
    }

    fn post_exec(
        &mut self,
        _fuzzer: &mut Z,
        _state: &mut S,
        _mgr: &mut EM,
        _input: &I,
    ) -> Result<(), Error> {
        self.last_runtime = current_time().checked_sub(self.start_time);
        Ok(())
    }
}

impl Named for TimeObserver {
    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(feature = "std")]
#[cfg(test)]
mod tests {

    use crate::{
        bolts::tuples::{tuple_list, tuple_list_type, Named},
        observers::{StdMapObserver, TimeObserver},
    };

    static mut MAP: [u32; 4] = [0; 4];

    #[test]
    fn test_observer_serde() {
        let obv = tuple_list!(
            TimeObserver::new("time"),
            StdMapObserver::new("map", unsafe { &mut MAP })
        );
        let vec = postcard::to_allocvec(&obv).unwrap();
        println!("{:?}", vec);
        let obv2: tuple_list_type!(TimeObserver, StdMapObserver<u32>) =
            postcard::from_bytes(&vec).unwrap();
        assert_eq!(obv.0.name(), obv2.0.name());
    }
}
