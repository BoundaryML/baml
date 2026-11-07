//! Generational task slab. Ready queues, waiters and late I/O messages carry
//! the full generation: reusing a slot cannot wake or complete a different task.
use super::Task;
#[derive(Default)]
pub(super) struct Tasks {
    slots: Vec<Slot>,
    free: Vec<usize>,
    len: usize,
}
pub(super) struct Slot {
    generation: u32,
    entry: Option<(usize, Task)>,
}
impl Tasks {
    pub fn reserve(&mut self) -> usize {
        let index = self.free.pop().unwrap_or_else(|| {
            let index = self.slots.len();
            assert!(index <= u32::MAX as usize);
            self.slots.push(Slot {
                generation: 0,
                entry: None,
            });
            index
        });
        ((self.slots[index].generation as usize) << 32) | index
    }
    pub fn insert(&mut self, id: usize, task: Task) {
        let slot = &mut self.slots[id as u32 as usize];
        assert!(slot.entry.is_none());
        slot.entry = Some((id, task));
        self.len += 1;
    }
    pub fn get(&self, id: &usize) -> Option<&Task> {
        let (key, task) = self.slots.get(*id as u32 as usize)?.entry.as_ref()?;
        (*key == *id).then_some(task)
    }
    pub fn get_mut(&mut self, id: &usize) -> Option<&mut Task> {
        let (key, task) = self.slots.get_mut(*id as u32 as usize)?.entry.as_mut()?;
        (*key == *id).then_some(task)
    }
    pub fn remove(&mut self, id: &usize) -> Option<Task> {
        let index = *id as u32 as usize;
        let slot = self.slots.get_mut(index)?;
        if slot.entry.as_ref()?.0 != *id {
            return None;
        }
        let (_, task) = slot.entry.take().unwrap();
        self.recycle(index);
        self.len -= 1;
        Some(task)
    }
    fn recycle(&mut self, index: usize) {
        // A hot slot can reach 2^32 reuses in a long-lived loop. Retire it
        // instead of wrapping its generation and aliasing an old message/id.
        if let Some(generation) = self.slots[index].generation.checked_add(1) {
            self.slots[index].generation = generation;
            self.free.push(index);
        }
    }
    pub fn drain(&mut self) -> Vec<(usize, Task)> {
        let mut tasks = Vec::with_capacity(self.len);
        for index in 0..self.slots.len() {
            if let Some(task) = self.slots[index].entry.take() {
                tasks.push(task);
                self.recycle(index);
            }
        }
        self.len = 0;
        tasks
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn values(&self) -> impl Iterator<Item = &Task> {
        self.slots
            .iter()
            .filter_map(|s| s.entry.as_ref().map(|(_, t)| t))
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&usize, &mut Task)> {
        self.slots
            .iter_mut()
            .filter_map(|s| s.entry.as_mut().map(|(id, t)| (&*id, t)))
    }
    pub fn keys(&self) -> impl Iterator<Item = &usize> {
        self.slots
            .iter()
            .filter_map(|s| s.entry.as_ref().map(|(id, _)| id))
    }
}
impl std::ops::Index<&usize> for Tasks {
    type Output = Task;
    fn index(&self, id: &usize) -> &Task {
        self.get(id).expect("live task")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_generation_retires_slot() {
        let mut tasks = Tasks::default();
        let id = tasks.reserve();
        let index = id as u32 as usize;
        tasks.slots[index].generation = u32::MAX;
        tasks.recycle(index);
        assert_ne!(tasks.reserve() as u32 as usize, index);
    }
}
