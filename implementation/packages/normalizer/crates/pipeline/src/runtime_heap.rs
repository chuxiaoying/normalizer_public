//! Stable symbolic identities for local allocations and borrowed objects.
//! Field values and source paths are tracked by the runtime stores.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct HeapRef(i64);

impl HeapRef {
    pub(crate) fn id(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Heap {
    next_local: i64,
    next_path: i64,
}

impl Heap {
    pub(crate) fn new() -> Self {
        Self {
            next_local: 1,
            next_path: -1,
        }
    }

    pub(crate) fn new_object(&mut self) -> HeapRef {
        let reference = HeapRef(self.next_local);
        self.next_local += 1;
        reference
    }

    pub(crate) fn new_path_object(&mut self) -> HeapRef {
        let reference = HeapRef(self.next_path);
        self.next_path -= 1;
        reference
    }
}
