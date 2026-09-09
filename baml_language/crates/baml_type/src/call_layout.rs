//! Argument slots at a call site, independent of value types and callee storage.
use crate::{FunctionParamMode, Name};
use borsh::{BorshDeserialize, BorshSerialize};

/// `None` is a required positional slot; `Some(name)` is an optional named
/// slot. An omitted optional still occupies its caller slot as an omission
/// sentinel. Parameter names on required slots have no calling significance.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CallLayout(pub Vec<Option<Name>>);

impl CallLayout {
    pub fn positional(count: usize) -> Self {
        Self(vec![None; count])
    }
    pub fn from_modes(params: impl IntoIterator<Item = (Option<Name>, FunctionParamMode)>) -> Self {
        Self(
            params
                .into_iter()
                .map(|(name, mode)| match mode {
                    FunctionParamMode::Required => None,
                    FunctionParamMode::Optional => {
                        Some(name.expect("optional parameter has a name"))
                    }
                })
                .collect(),
        )
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// For each target slot, identify its caller slot or an omitted default.
    /// Validate the complete map before any caller value is consumed.
    pub fn map_to(&self, target: &Self) -> Result<Vec<Option<usize>>, String> {
        let names = |layout: &Self| {
            let mut names = std::collections::HashSet::new();
            for name in layout.0.iter().flatten() {
                if !names.insert(name.clone()) {
                    return Err(format!("duplicate optional parameter `{name}`"));
                }
            }
            Ok(names)
        };
        let caller_names = names(self)?;
        let target_names = names(target)?;
        if !caller_names.is_subset(&target_names) {
            return Err("call supplies an unsupported optional parameter".into());
        }
        let mut required = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, name)| name.is_none())
            .map(|(index, _)| index);
        let mut map = Vec::with_capacity(target.len());
        for slot in &target.0 {
            map.push(match slot {
                None => Some(
                    required
                        .next()
                        .ok_or("call is missing a required argument")?,
                ),
                Some(name) => self.0.iter().position(|slot| slot.as_ref() == Some(name)),
            });
        }
        if required.next().is_some() {
            return Err("call has extra required arguments".into());
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout(slots: &[Option<&str>]) -> CallLayout {
        CallLayout(slots.iter().map(|slot| slot.map(Name::new)).collect())
    }
    #[test]
    fn reorders_named_slots_and_omits_implementation_extras() {
        let caller = layout(&[None, Some("prefix"), Some("suffix")]);
        let target = layout(&[None, Some("suffix"), Some("extra"), Some("prefix")]);
        assert_eq!(
            caller.map_to(&target).unwrap(),
            vec![Some(0), Some(2), None, Some(1)]
        );
        assert_eq!(
            CallLayout::positional(1).map_to(&target).unwrap(),
            vec![Some(0), None, None, None]
        );
    }
    #[test]
    fn invalid_shapes_fail_before_mapping_values() {
        assert!(
            CallLayout::positional(2)
                .map_to(&CallLayout::positional(1))
                .is_err()
        );
        assert!(
            CallLayout::positional(1)
                .map_to(&CallLayout::positional(2))
                .is_err()
        );
        assert!(layout(&[Some("a")]).map_to(&layout(&[Some("b")])).is_err());
        assert!(
            layout(&[Some("a"), Some("a")])
                .map_to(&layout(&[Some("a")]))
                .is_err()
        );
    }
}
