//! The `CmpObserver` provides access to the logged values of CMP instructions

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    bolts::{ownedref::OwnedRefMut, tuples::Named, AsSlice},
    executors::HasExecHooks,
    observers::Observer,
    state::HasMetadata,
    Error,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum CmpValues {
    U8((u8, u8)),
    U16((u16, u16)),
    U32((u32, u32)),
    U64((u64, u64)),
    Bytes((Vec<u8>, Vec<u8>)),
}

impl CmpValues {
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            CmpValues::U8(_) | CmpValues::U16(_) | CmpValues::U32(_) | CmpValues::U64(_)
        )
    }

    #[must_use]
    pub fn to_u64_tuple(&self) -> Option<(u64, u64)> {
        match self {
            CmpValues::U8(t) => Some((u64::from(t.0), u64::from(t.1))),
            CmpValues::U16(t) => Some((u64::from(t.0), u64::from(t.1))),
            CmpValues::U32(t) => Some((u64::from(t.0), u64::from(t.1))),
            CmpValues::U64(t) => Some(*t),
            CmpValues::Bytes(_) => None,
        }
    }
}

/// A state metadata holding a list of values logged from comparisons
#[derive(Default, Serialize, Deserialize)]
pub struct CmpValuesMetadata {
    /// A `list` of values.
    pub list: Vec<CmpValues>,
}

crate::impl_serdeany!(CmpValuesMetadata);

impl AsSlice<CmpValues> for CmpValuesMetadata {
    /// Convert to a slice
    #[must_use]
    fn as_slice(&self) -> &[CmpValues] {
        self.list.as_slice()
    }
}

impl CmpValuesMetadata {
    /// Creates a new [`struct@CmpValuesMetadata`]
    #[must_use]
    pub fn new() -> Self {
        Self { list: vec![] }
    }
}

/// A [`CmpMap`] traces comparisons during the current execution
pub trait CmpMap: Serialize + DeserializeOwned {
    /// Get the number of cmps
    fn len(&self) -> usize;

    /// Get if it is empty
    #[must_use]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // Get the number of executions for a cmp
    fn executions_for(&self, idx: usize) -> usize;

    // Get the number of logged executions for a cmp
    fn usable_executions_for(&self, idx: usize) -> usize;

    // Get the logged values for a cmp
    fn values_of(&self, idx: usize, execution: usize) -> CmpValues;

    /// Reset the state
    fn reset(&mut self) -> Result<(), Error>;
}

/// A [`CmpObserver`] observes the traced comparisons during the current execution using a [`CmpMap`]
pub trait CmpObserver<CM>: Observer
where
    CM: CmpMap,
{
    /// Get the number of usable cmps (all by default)
    fn usable_count(&self) -> usize;

    /// Get the `CmpMap`
    fn map(&self) -> &CM;

    /// Get the `CmpMap` (mut)
    fn map_mut(&mut self) -> &mut CM;

    /// Add [`CmpValuesMetadata`] to the State including the logged values.
    /// This routine does a basic loop filtering because loop index cmps are not interesting.
    fn add_cmpvalues_meta<S>(&mut self, state: &mut S)
    where
        S: HasMetadata,
    {
        if state.metadata().get::<CmpValuesMetadata>().is_none() {
            state.add_metadata(CmpValuesMetadata::new());
        }
        let meta = state.metadata_mut().get_mut::<CmpValuesMetadata>().unwrap();
        meta.list.clear();
        let count = self.usable_count();
        for i in 0..count {
            let execs = self.map().usable_executions_for(i);
            if execs > 0 {
                // Recongize loops and discard if needed
                if execs > 4 {
                    let mut increasing_v0 = 0;
                    let mut increasing_v1 = 0;
                    let mut decreasing_v0 = 0;
                    let mut decreasing_v1 = 0;

                    let mut last: Option<CmpValues> = None;
                    for j in 0..execs {
                        let val = self.map().values_of(i, j);
                        if let Some(l) = last.and_then(|x| x.to_u64_tuple()) {
                            if let Some(v) = val.to_u64_tuple() {
                                if l.0.wrapping_add(1) == v.0 {
                                    increasing_v0 += 1;
                                }
                                if l.1.wrapping_add(1) == v.1 {
                                    increasing_v1 += 1;
                                }
                                if l.0.wrapping_sub(1) == v.0 {
                                    decreasing_v0 += 1;
                                }
                                if l.1.wrapping_sub(1) == v.1 {
                                    decreasing_v1 += 1;
                                }
                            }
                        }
                        last = Some(val);
                    }
                    // We check for execs-2 because the logged execs may wrap and have something like
                    // 8 9 10 3 4 5 6 7
                    if increasing_v0 >= execs - 2
                        || increasing_v1 >= execs - 2
                        || decreasing_v0 >= execs - 2
                        || decreasing_v1 >= execs - 2
                    {
                        continue;
                    }
                }
                for j in 0..execs {
                    meta.list.push(self.map().values_of(i, j));
                }
            }
        }
    }
}

/// A standard [`CmpObserver`] observer
#[derive(Serialize, Deserialize, Debug)]
#[serde(bound = "CM: serde::de::DeserializeOwned")]
pub struct StdCmpObserver<'a, CM>
where
    CM: CmpMap,
{
    map: OwnedRefMut<'a, CM>,
    size: Option<OwnedRefMut<'a, usize>>,
    name: String,
}

impl<'a, CM> CmpObserver<CM> for StdCmpObserver<'a, CM>
where
    CM: CmpMap,
{
    /// Get the number of usable cmps (all by default)
    fn usable_count(&self) -> usize {
        match &self.size {
            None => self.map().len(),
            Some(o) => *o.as_ref(),
        }
    }

    fn map(&self) -> &CM {
        self.map.as_ref()
    }

    fn map_mut(&mut self) -> &mut CM {
        self.map.as_mut()
    }
}

impl<'a, CM> Observer for StdCmpObserver<'a, CM> where CM: CmpMap {}

impl<'a, CM, EM, I, S, Z> HasExecHooks<EM, I, S, Z> for StdCmpObserver<'a, CM>
where
    CM: CmpMap,
{
    fn pre_exec(
        &mut self,
        _fuzzer: &mut Z,
        _state: &mut S,
        _mgr: &mut EM,
        _input: &I,
    ) -> Result<(), Error> {
        self.map.as_mut().reset()?;
        Ok(())
    }
}

impl<'a, CM> Named for StdCmpObserver<'a, CM>
where
    CM: CmpMap,
{
    fn name(&self) -> &str {
        &self.name
    }
}

impl<'a, CM> StdCmpObserver<'a, CM>
where
    CM: CmpMap,
{
    /// Creates a new [`StdCmpObserver`] with the given name and map.
    #[must_use]
    pub fn new(name: &'static str, map: &'a mut CM) -> Self {
        Self {
            name: name.to_string(),
            size: None,
            map: OwnedRefMut::Ref(map),
        }
    }

    /// Creates a new [`StdCmpObserver`] with the given name, map and reference to variable size.
    #[must_use]
    pub fn with_size(name: &'static str, map: &'a mut CM, size: &'a mut usize) -> Self {
        Self {
            name: name.to_string(),
            size: Some(OwnedRefMut::Ref(size)),
            map: OwnedRefMut::Ref(map),
        }
    }
}
