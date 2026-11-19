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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Models,
    Shadows,
}

/// BindTexture selects _rename_ring_tex_0 or _rename_ring_tex_1 by pass.
/// DrawShadow submits the original _rename_ring_tex_2 model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Draw {
    FogEnabled(bool),
    ApplyFog,
    BindTexture(Pass),
    Push,
    Pop,
    Translate([f32; 3]),
    RotateX(i32),
    RotateY(i32),
    RotateZ(i32),
    Scale([f32; 3]),
    DrawModel(Mesh),
    DrawShadow,
}

impl Manager {
    /// Run the scheduled RingModelDisp or RingModelDisp2 with the host's current
    /// view transform. The two native callbacks may be scheduled separately.
    pub fn display(
        &self,
        pass: Pass,
        disabled: bool,
        disable_object_fog: bool,
        view_z: impl FnMut([f32; 3]) -> f32,
        mut emit: impl FnMut(Draw),
    ) {
        use Draw::*;
        if disabled || !self.task {
            return;
        }
        if disable_object_fog {
            emit(FogEnabled(false));
            emit(ApplyFog);
        }
        emit(BindTexture(pass));
        emit(Push);
        match pass {
            Pass::Models => self.models(false, view_z, |_, m, mesh| {
                for op in [
                    Translate(m.position),
                    RotateY(m.yaw),
                    DrawModel(mesh),
                    Pop,
                    Push,
                ] {
                    emit(op);
                }
            }),
            Pass::Shadows => self.shadows(false, |_, m| {
                let [x, y, z] = m.shadow_angles;
                for op in [
                    Translate([m.position[0], m.shadow_y, m.position[2]]),
                    RotateZ(z),
                    RotateX(x),
                    RotateY(y),
                    Translate([0., 0.5, 0.]),
                    Scale([0.8, 1., 0.4]),
                    DrawShadow,
                    Pop,
                    Push,
                ] {
                    emit(op);
                }
            }),
        }
        emit(Pop);
        if disable_object_fog {
            emit(FogEnabled(true));
            emit(ApplyFog);
        }
    }
}
