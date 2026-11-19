//! GSNE8P o_ring.c shared model list. Task scheduling and view transforms belong to the host.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle(u64);

#[derive(Debug)]
pub struct Model {
    pub position: [f32; 3],
    pub yaw: i32,
    pub shadow_y: f32,
    pub shadow_angles: [i32; 3],
    pub hidden: bool,
    retired: bool,
}

#[derive(Default)]
pub struct Manager {
    next: u64,
    nodes: BTreeMap<Handle, Model>,
    task: bool,
    cleanup: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mesh {
    RingL1,
    RingL2,
    Ring,
}

impl Manager {
    /// Call after successful ringwk allocation. A failed manager-task allocation does
    /// not discard the model; CreateRingObject retries on a later successful allocation.
    pub fn create(&mut self, allocate_task: impl FnOnce() -> bool) -> Handle {
        if !self.task {
            self.task = allocate_task();
        }
        let id = Handle(self.next);
        self.next = self
            .next
            .checked_add(1)
            .expect("ring model handle exhaustion");
        self.nodes.insert(
            id,
            Model {
                position: [0.; 3],
                yaw: 0,
                shadow_y: -1_000_000.,
                shadow_angles: [0; 3],
                hidden: false,
                retired: false,
            },
        );
        id
    }
    pub fn model_mut(&mut self, id: Handle) -> Option<&mut Model> {
        self.nodes.get_mut(&id).filter(|n| !n.retired)
    }
    /// RingEnd hides the model now; RingModelExec unlinks it on its own update.
    pub fn retire(&mut self, id: Handle) {
        if let Some(n) = self.nodes.get_mut(&id) {
            n.hidden = true;
            n.retired = true;
            self.cleanup = true;
        }
    }
    /// Returns the native DestroyTask request; destruction occurs in the destructor.
    pub fn execute(&mut self) -> bool {
        if !self.task {
            return false;
        }
        if self.cleanup {
            self.nodes.retain(|_, n| !n.retired);
            self.cleanup = false;
        }
        self.nodes.is_empty()
    }
    pub fn destroy(&mut self) {
        self.nodes.clear();
        self.cleanup = false;
        self.task = false;
    }
    /// RingModelDisp selects by negative transformed Z, newest model first.
    pub fn models(
        &self,
        display_disabled: bool,
        mut view_z: impl FnMut([f32; 3]) -> f32,
        mut submit: impl FnMut(Handle, &Model, Mesh),
    ) {
        if display_disabled || !self.task {
            return;
        }
        for (&id, n) in self.nodes.iter().rev().filter(|(_, n)| !n.hidden) {
            let depth = -view_z(n.position);
            if depth > -10. {
                submit(
                    id,
                    n,
                    if depth < 65. {
                        Mesh::RingL1
                    } else if depth < 130. {
                        Mesh::RingL2
                    } else {
                        Mesh::Ring
                    },
                );
            }
        }
    }
    /// The delayed shadow pass tests the two sentinel heights independently of mesh visibility.
    pub fn shadows(&self, display_disabled: bool, mut submit: impl FnMut(Handle, &Model)) {
        if display_disabled || !self.task {
            return;
        }
        for (&id, n) in self.nodes.iter().rev() {
            if n.shadow_y != -1_000_000. && n.shadow_y != 1_000_000. {
                submit(id, n);
            }
        }
    }
}
