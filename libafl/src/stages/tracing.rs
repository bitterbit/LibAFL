use core::{marker::PhantomData, mem::drop};

use crate::{
    corpus::Corpus,
    executors::{Executor, HasExecHooksTuple, HasObservers, HasObserversHooks, ShadowExecutor},
    inputs::Input,
    mark_feature_time,
    observers::ObserversTuple,
    stages::Stage,
    start_timer,
    state::{HasClientPerfStats, HasCorpus, HasExecutions},
    Error,
};

#[cfg(feature = "introspection")]
use crate::stats::PerfFeature;

/// A stage that runs a tracer executor
#[derive(Clone, Debug)]
pub struct TracingStage<C, EM, I, OT, S, TE, Z>
where
    I: Input,
    C: Corpus<I>,
    TE: Executor<EM, I, S, Z> + HasObservers<OT> + HasObserversHooks<EM, I, OT, S, Z>,
    OT: ObserversTuple + HasExecHooksTuple<EM, I, S, Z>,
    S: HasClientPerfStats + HasExecutions + HasCorpus<C, I>,
{
    tracer_executor: TE,
    #[allow(clippy::type_complexity)]
    phantom: PhantomData<(C, EM, I, OT, S, TE, Z)>,
}

impl<E, C, EM, I, OT, S, TE, Z> Stage<E, EM, S, Z> for TracingStage<C, EM, I, OT, S, TE, Z>
where
    I: Input,
    C: Corpus<I>,
    TE: Executor<EM, I, S, Z> + HasObservers<OT> + HasObserversHooks<EM, I, OT, S, Z>,
    OT: ObserversTuple + HasExecHooksTuple<EM, I, S, Z>,
    S: HasClientPerfStats + HasExecutions + HasCorpus<C, I>,
{
    #[inline]
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        _executor: &mut E,
        state: &mut S,
        manager: &mut EM,
        corpus_idx: usize,
    ) -> Result<(), Error> {
        start_timer!(state);
        let input = state
            .corpus()
            .get(corpus_idx)?
            .borrow_mut()
            .load_input()?
            .clone();
        mark_feature_time!(state, PerfFeature::GetInputFromCorpus);

        start_timer!(state);
        self.tracer_executor
            .pre_exec_observers(fuzzer, state, manager, &input)?;
        mark_feature_time!(state, PerfFeature::PreExecObservers);

        start_timer!(state);
        drop(
            self.tracer_executor
                .run_target(fuzzer, state, manager, &input)?,
        );
        mark_feature_time!(state, PerfFeature::TargetExecution);

        *state.executions_mut() += 1;

        start_timer!(state);
        self.tracer_executor
            .post_exec_observers(fuzzer, state, manager, &input)?;
        mark_feature_time!(state, PerfFeature::PostExecObservers);

        Ok(())
    }
}

impl<C, EM, I, OT, S, TE, Z> TracingStage<C, EM, I, OT, S, TE, Z>
where
    I: Input,
    C: Corpus<I>,
    TE: Executor<EM, I, S, Z> + HasObservers<OT> + HasObserversHooks<EM, I, OT, S, Z>,
    OT: ObserversTuple + HasExecHooksTuple<EM, I, S, Z>,
    S: HasClientPerfStats + HasExecutions + HasCorpus<C, I>,
{
    /// Creates a new default stage
    pub fn new(tracer_executor: TE) -> Self {
        Self {
            tracer_executor,
            phantom: PhantomData,
        }
    }
}

/// A stage that runs the shadow executor using also the shadow observers
#[derive(Clone, Debug)]
pub struct ShadowTracingStage<C, E, EM, I, OT, S, SOT, Z> {
    #[allow(clippy::type_complexity)]
    phantom: PhantomData<(C, E, EM, I, OT, S, SOT, Z)>,
}

impl<C, E, EM, I, OT, S, SOT, Z> Stage<ShadowExecutor<E, SOT>, EM, S, Z>
    for ShadowTracingStage<C, E, EM, I, OT, S, SOT, Z>
where
    I: Input,
    C: Corpus<I>,
    E: Executor<EM, I, S, Z> + HasObservers<OT> + HasObserversHooks<EM, I, OT, S, Z>,
    OT: ObserversTuple + HasExecHooksTuple<EM, I, S, Z>,
    SOT: ObserversTuple + HasExecHooksTuple<EM, I, S, Z>,
    S: HasClientPerfStats + HasExecutions + HasCorpus<C, I>,
{
    #[inline]
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut ShadowExecutor<E, SOT>,
        state: &mut S,
        manager: &mut EM,
        corpus_idx: usize,
    ) -> Result<(), Error> {
        start_timer!(state);
        let input = state
            .corpus()
            .get(corpus_idx)?
            .borrow_mut()
            .load_input()?
            .clone();
        mark_feature_time!(state, PerfFeature::GetInputFromCorpus);

        let prev_shadow_hooks = *executor.shadow_hooks();
        *executor.shadow_hooks_mut() = true;

        start_timer!(state);
        executor.pre_exec_observers(fuzzer, state, manager, &input)?;
        mark_feature_time!(state, PerfFeature::PreExecObservers);

        start_timer!(state);
        drop(executor.run_target(fuzzer, state, manager, &input)?);
        mark_feature_time!(state, PerfFeature::TargetExecution);

        *state.executions_mut() += 1;

        start_timer!(state);
        executor.post_exec_observers(fuzzer, state, manager, &input)?;
        mark_feature_time!(state, PerfFeature::PostExecObservers);

        *executor.shadow_hooks_mut() = prev_shadow_hooks;

        Ok(())
    }
}

impl<C, E, EM, I, OT, S, SOT, Z> ShadowTracingStage<C, E, EM, I, OT, S, SOT, Z>
where
    I: Input,
    C: Corpus<I>,
    E: Executor<EM, I, S, Z> + HasObservers<OT> + HasObserversHooks<EM, I, OT, S, Z>,
    OT: ObserversTuple + HasExecHooksTuple<EM, I, S, Z>,
    SOT: ObserversTuple + HasExecHooksTuple<EM, I, S, Z>,
    S: HasClientPerfStats + HasExecutions + HasCorpus<C, I>,
{
    /// Creates a new default stage
    pub fn new(_executor: &mut ShadowExecutor<E, SOT>) -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}
