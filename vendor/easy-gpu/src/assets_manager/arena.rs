use crate::assets_manager::handle::Handle;

struct Slot<T> {
    value: Option<T>,
    generation: u32,
}

pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self {
            slots: vec![],
            free: vec![],
        }
    }
    pub fn insert(&mut self, value: T) -> Handle<T> {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);

            Handle {
                index,
                generation: slot.generation,
                _marker: std::marker::PhantomData,
            }
        } else {
            let index = self.slots.len() as u32;

            self.slots.push(Slot {
                value: Some(value),
                generation: 0,
            });

            Handle {
                index,
                generation: 0,
                _marker: std::marker::PhantomData,
            }
        }
    }

    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        let slot = self.slots.get(handle.index as usize)?;

        if slot.generation == handle.generation {
            slot.value.as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let slot = self.slots.get_mut(handle.index as usize)?;

        if slot.generation == handle.generation {
            slot.value.as_mut()
        } else {
            None
        }
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let slot = self.slots.get_mut(handle.index as usize)?;

        if slot.generation != handle.generation {
            return None;
        }

        let value = slot.value.take();
        slot.generation += 1;

        self.free.push(handle.index);

        value
    }
}
