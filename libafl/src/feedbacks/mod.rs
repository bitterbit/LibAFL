//! The feedbacks reduce observer state after each run to a single `is_interesting`-value.
//! If a testcase is interesting, it may be added to a Corpus.

pub mod map;
pub use map::*;

use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};

use crate::{
    bolts::tuples::{MatchName, Named},
    corpus::Testcase,
    events::EventFirer,
    executors::ExitKind,
    inputs::Input,
    observers::{ObserversTuple, TimeObserver},
    Error,
};

#[cfg(feature = "introspection")]
use crate::stats::NUM_FEEDBACKS;

use core::{marker::PhantomData, time::Duration};

/// Feedbacks evaluate the observers.
/// Basically, they reduce the information provided by an observer to a value,
/// indicating the "interestingness" of the last run.
pub trait Feedback<I, S>: Named
where
    I: Input,
{
    /// `is_interesting ` return if an input is worth the addition to the corpus
    fn is_interesting<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple;

    #[cfg(feature = "introspection")]
    #[allow(clippy::too_many_arguments)]
    fn is_interesting_with_perf<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
        feedback_stats: &mut [u64; NUM_FEEDBACKS],
        feedback_index: usize,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        // Start a timer for this feedback
        let start_time = crate::cpu::read_time_counter();

        // Execute this feedback
        let ret = self.is_interesting(state, manager, input, observers, &exit_kind);

        // Get the elapsed time for checking this feedback
        let elapsed = crate::cpu::read_time_counter() - start_time;

        // TODO: A more meaningful way to get perf for each feedback

        // Add this stat to the feedback metrics
        feedback_stats[feedback_index] = elapsed;

        ret
    }

    /// Append to the testcase the generated metadata in case of a new corpus item
    #[inline]
    fn append_metadata(
        &mut self,
        _state: &mut S,
        _testcase: &mut Testcase<I>,
    ) -> Result<(), Error> {
        Ok(())
    }

    /// Discard the stored metadata in case that the testcase is not added to the corpus
    #[inline]
    fn discard_metadata(&mut self, _state: &mut S, _input: &I) -> Result<(), Error> {
        Ok(())
    }
}

/// [`FeedbackState`] is the data associated with a [`Feedback`] that must persist as part
/// of the fuzzer State
pub trait FeedbackState: Named + serde::Serialize + serde::de::DeserializeOwned {}

/// A haskell-style tuple of feedback states
pub trait FeedbackStatesTuple: MatchName + serde::Serialize + serde::de::DeserializeOwned {}

impl FeedbackStatesTuple for () {}

impl<Head, Tail> FeedbackStatesTuple for (Head, Tail)
where
    Head: FeedbackState,
    Tail: FeedbackStatesTuple,
{
}

pub struct FeedbackTuple<A, B, I, S, FL>
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    FL: FeedbackLogic<A, B, I, S>,
    I: Input,
{
    pub head: A,
    pub tail: B,
    name: String,
    phantom: PhantomData<(I, S, FL)>,
}

impl<A, B, I, S, FL> Named for FeedbackTuple<A, B, I, S, FL>
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    FL: FeedbackLogic<A, B, I, S>,
    I: Input,
{
    fn name(&self) -> &str {
        self.name.as_ref()
    }
}

impl<A, B, I, S, FL> FeedbackTuple<A, B, I, S, FL>
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    FL: FeedbackLogic<A, B, I, S>,
    I: Input,
{
    pub fn new(head: A, tail: B) -> Self {
        let name = format!("{} ({},{})", FL::name(), head.name(), tail.name());
        Self {
            head,
            tail,
            name,
            phantom: PhantomData,
        }
    }
}

impl<A, B, I, S, FL> Feedback<I, S> for FeedbackTuple<A, B, I, S, FL>
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    FL: FeedbackLogic<A, B, I, S>,
    I: Input,
{
    fn is_interesting<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        FL::is_pair_interesting(
            &mut self.head,
            &mut self.tail,
            state,
            manager,
            input,
            observers,
            exit_kind,
        )
    }

    #[cfg(feature = "introspection")]
    fn is_interesting_with_perf<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
        feedback_stats: &mut [u64; NUM_FEEDBACKS],
        feedback_index: usize,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        FL::is_pair_interesting_with_perf(
            &mut self.head,
            &mut self.tail,
            state,
            manager,
            input,
            observers,
            exit_kind,
            feedback_stats,
            feedback_index,
        )
    }

    #[inline]
    fn append_metadata(&mut self, state: &mut S, testcase: &mut Testcase<I>) -> Result<(), Error> {
        self.head.append_metadata(state, testcase)?;
        self.tail.append_metadata(state, testcase)
    }

    #[inline]
    fn discard_metadata(&mut self, state: &mut S, input: &I) -> Result<(), Error> {
        self.head.discard_metadata(state, input)?;
        self.tail.discard_metadata(state, input)
    }
}

pub trait FeedbackLogic<A, B, I, S>: 'static
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    I: Input,
{
    fn name() -> &'static str;

    fn is_pair_interesting<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple;

    #[cfg(feature = "introspection")]
    fn is_pair_interesting_with_perf<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
        feedback_stats: &mut [u64; NUM_FEEDBACKS],
        feedback_index: usize,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple;
}

pub struct LogicEagerOr {}
pub struct LogicFastOr {}
pub struct LogicAnd {}

impl<A, B, I, S> FeedbackLogic<A, B, I, S> for LogicEagerOr
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    I: Input,
{
    fn name() -> &'static str {
        "Eager OR"
    }

    fn is_pair_interesting<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        let a = first.is_interesting(state, manager, input, observers, exit_kind)?;
        let b = second.is_interesting(state, manager, input, observers, exit_kind)?;
        Ok(a || b)
    }

    #[cfg(feature = "introspection")]
    fn is_pair_interesting_with_perf<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
        feedback_stats: &mut [u64; NUM_FEEDBACKS],
        feedback_index: usize,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        // Execute this feedback
        let a = first.is_interesting_with_perf(
            state,
            manager,
            input,
            observers,
            &exit_kind,
            feedback_stats,
            feedback_index,
        )?;

        let b = second.is_interesting_with_perf(
            state,
            manager,
            input,
            observers,
            &exit_kind,
            feedback_stats,
            feedback_index + 1,
        )?;
        Ok(a || b)
    }
}

impl<A, B, I, S> FeedbackLogic<A, B, I, S> for LogicFastOr
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    I: Input,
{
    fn name() -> &'static str {
        "Fast OR"
    }

    fn is_pair_interesting<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        let a = first.is_interesting(state, manager, input, observers, exit_kind)?;
        if a {
            return Ok(true);
        }

        second.is_interesting(state, manager, input, observers, exit_kind)
    }

    #[cfg(feature = "introspection")]
    fn is_pair_interesting_with_perf<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
        feedback_stats: &mut [u64; NUM_FEEDBACKS],
        feedback_index: usize,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        // Execute this feedback
        let a = first.is_interesting_with_perf(
            state,
            manager,
            input,
            observers,
            &exit_kind,
            feedback_stats,
            feedback_index,
        )?;

        if a {
            return Ok(true);
        }

        second.is_interesting_with_perf(
            state,
            manager,
            input,
            observers,
            &exit_kind,
            feedback_stats,
            feedback_index + 1,
        )
    }
}

impl<A, B, I, S> FeedbackLogic<A, B, I, S> for LogicAnd
where
    A: Feedback<I, S>,
    B: Feedback<I, S>,
    I: Input,
{
    fn name() -> &'static str {
        "AND"
    }

    fn is_pair_interesting<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        let a = first.is_interesting(state, manager, input, observers, exit_kind)?;
        let b = second.is_interesting(state, manager, input, observers, exit_kind)?;
        Ok(a && b)
    }

    #[cfg(feature = "introspection")]
    fn is_pair_interesting_with_perf<EM, OT>(
        first: &mut A,
        second: &mut B,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
        feedback_stats: &mut [u64; NUM_FEEDBACKS],
        feedback_index: usize,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        // Execute this feedback
        let a = first.is_interesting_with_perf(
            state,
            manager,
            input,
            observers,
            &exit_kind,
            feedback_stats,
            feedback_index,
        )?;

        let b = second.is_interesting_with_perf(
            state,
            manager,
            input,
            observers,
            &exit_kind,
            feedback_stats,
            feedback_index + 1,
        )?;
        Ok(a && b)
    }
}

/// Compose feedbacks with an AND operation
pub type AndFeedback<A, B, I, S> = FeedbackTuple<A, B, I, S, LogicAnd>;

/// Compose feedbacks with an Eager-OR operation
/// Eager stands for trying to compute all feedbacks before returning
/// This means that all feedbacks functions are called even if a result can already be concluded
pub type EagerOrFeedback<A, B, I, S> = FeedbackTuple<A, B, I, S, LogicEagerOr>;

/// Compose feedbacks with an Fast-OR operation
/// Fast OR stops computing feedbacks after finding the first positive feedback
/// This means any feedback that is not first might be skipped, use caution when using with
/// `TimeFeedback`
pub type FastOrFeedback<A, B, I, S> = FeedbackTuple<A, B, I, S, LogicFastOr>;

/// Compose feedbacks with an OR operation
pub struct NotFeedback<A, I, S>
where
    A: Feedback<I, S>,
    I: Input,
{
    /// The feedback to invert
    pub first: A,
    /// The name
    name: String,
    phantom: PhantomData<(I, S)>,
}

impl<A, I, S> Feedback<I, S> for NotFeedback<A, I, S>
where
    A: Feedback<I, S>,
    I: Input,
{
    fn is_interesting<EM, OT>(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        Ok(!self
            .first
            .is_interesting(state, manager, input, observers, exit_kind)?)
    }

    #[inline]
    fn append_metadata(&mut self, state: &mut S, testcase: &mut Testcase<I>) -> Result<(), Error> {
        self.first.append_metadata(state, testcase)
    }

    #[inline]
    fn discard_metadata(&mut self, state: &mut S, input: &I) -> Result<(), Error> {
        self.first.discard_metadata(state, input)
    }
}

impl<A, I, S> Named for NotFeedback<A, I, S>
where
    A: Feedback<I, S>,
    I: Input,
{
    #[inline]
    fn name(&self) -> &str {
        &self.name
    }
}

impl<A, I, S> NotFeedback<A, I, S>
where
    A: Feedback<I, S>,
    I: Input,
{
    /// Creates a new [`NotFeedback`].
    pub fn new(first: A) -> Self {
        let name = format!("Not({})", first.name());
        Self {
            first,
            name,
            phantom: PhantomData,
        }
    }
}

/// Variadic macro to create a chain of AndFeedback
#[macro_export]
macro_rules! feedback_and {
    ( $last:expr ) => { $last };

    ( $head:expr, $($tail:expr), +) => {
        // recursive call
        $crate::feedbacks::AndFeedback::new($head , feedback_and!($($tail),+))
    };
}

/// Variadic macro to create a chain of OrFeedback
#[macro_export]
macro_rules! feedback_or_eager {
    ( $last:expr ) => { $last };

    ( $head:expr, $($tail:expr), +) => {
        // recursive call
        $crate::feedbacks::EagerOrFeedback::new($head , feedback_or_eager!($($tail),+))
    };
}

#[macro_export]
macro_rules! feedback_or_fast {
    ( $last:expr ) => { $last };

    ( $head:expr, $($tail:expr), +) => {
        // recursive call
        $crate::feedbacks::FastOrFeedback::new($head , feedback_or_fast!($($tail),+))
    };
}

/// Variadic macro to create a NotFeedback
#[macro_export]
macro_rules! feedback_not {
    ( $last:expr ) => {
        $crate::feedbacks::NotFeedback::new($last)
    };
}

/// Hack to use () as empty Feedback
impl<I, S> Feedback<I, S> for ()
where
    I: Input,
{
    fn is_interesting<EM, OT>(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        _observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        Ok(false)
    }
}

impl Named for () {
    #[inline]
    fn name(&self) -> &str {
        "Empty"
    }
}

/// A [`CrashFeedback`] reports as interesting if the target crashed.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CrashFeedback {}

impl<I, S> Feedback<I, S> for CrashFeedback
where
    I: Input,
{
    fn is_interesting<EM, OT>(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        _observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        if let ExitKind::Crash = exit_kind {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Named for CrashFeedback {
    #[inline]
    fn name(&self) -> &str {
        "CrashFeedback"
    }
}

impl CrashFeedback {
    /// Creates a new [`CrashFeedback`]
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for CrashFeedback {
    fn default() -> Self {
        Self::new()
    }
}

/// A [`TimeoutFeedback`] reduces the timeout value of a run.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeoutFeedback {}

impl<I, S> Feedback<I, S> for TimeoutFeedback
where
    I: Input,
{
    fn is_interesting<EM, OT>(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        _observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        if let ExitKind::Timeout = exit_kind {
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Named for TimeoutFeedback {
    #[inline]
    fn name(&self) -> &str {
        "TimeoutFeedback"
    }
}

impl TimeoutFeedback {
    /// Returns a new [`TimeoutFeedback`].
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for TimeoutFeedback {
    fn default() -> Self {
        Self::new()
    }
}

/// Nop feedback that annotates execution time in the new testcase, if any
/// for this Feedback, the testcase is never interesting (use with an OR)
/// It decides, if the given [`TimeObserver`] value of a run is interesting.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeFeedback {
    exec_time: Option<Duration>,
    name: String,
}

impl<I, S> Feedback<I, S> for TimeFeedback
where
    I: Input,
{
    fn is_interesting<EM, OT>(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error>
    where
        EM: EventFirer<I, S>,
        OT: ObserversTuple,
    {
        // TODO Replace with match_name_type when stable
        let observer = observers.match_name::<TimeObserver>(self.name()).unwrap();
        self.exec_time = *observer.last_runtime();
        Ok(false)
    }

    /// Append to the testcase the generated metadata in case of a new corpus item
    #[inline]
    fn append_metadata(&mut self, _state: &mut S, testcase: &mut Testcase<I>) -> Result<(), Error> {
        if let Some(exec_time) = self.exec_time {
            *testcase.exec_time_mut() = Some(exec_time);
        } else {
            return Err(Error::IllegalState("TimeFeedback is missing `exec_time` when append_metadata was called. 
                                       Make sure you are not using a `FeedbackOrFast` as it might skip `is_interesting`".to_string()));
        }

        self.exec_time = None;
        Ok(())
    }

    /// Discard the stored metadata in case that the testcase is not added to the corpus
    #[inline]
    fn discard_metadata(&mut self, _state: &mut S, _input: &I) -> Result<(), Error> {
        self.exec_time = None;
        Ok(())
    }
}

impl Named for TimeFeedback {
    #[inline]
    fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl TimeFeedback {
    /// Creates a new [`TimeFeedback`], deciding if the value of a [`TimeObserver`] with the given `name` of a run is interesting.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            exec_time: None,
            name: name.to_string(),
        }
    }

    /// Creates a new [`TimeFeedback`], deciding if the given [`TimeObserver`] value of a run is interesting.
    #[must_use]
    pub fn new_with_observer(observer: &TimeObserver) -> Self {
        Self {
            exec_time: None,
            name: observer.name().to_string(),
        }
    }
}
