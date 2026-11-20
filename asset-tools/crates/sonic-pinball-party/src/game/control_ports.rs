//! Native-address binding for input, both callback stacks and the initial
//! screen wrapper. All state remains in the supplied shared memory image.
use super::{
    control::{CallbackId, CallbackStack, InputState},
    frame::{FrameError, FrameMemory, NativeCall},
    lifecycle::LifecycleCalls,
    startup,
};
pub const MAIN: u32 = 0x03000e00;
pub const AUX: u32 = 0x03002060;
pub const INPUT: u32 = 0x03000510;
fn err(off: u32, why: impl ToString) -> FrameError {
    FrameError::Dependency {
        call: NativeCall {
            entry_offset: off,
            argument0: None,
        },
        reason: why.to_string(),
    }
}
fn read<const N: usize>(m: &dyn FrameMemory, base: u32) -> Result<[u16; N], FrameError> {
    let mut a = [0; N];
    for (i, v) in a.iter_mut().enumerate() {
        *v = m.read_u16(base + i as u32 * 2)?;
    }
    Ok(a)
}
fn write(
    m: &mut dyn FrameMemory,
    base: u32,
    before: &[u16],
    after: &[u16],
) -> Result<(), FrameError> {
    for (i, (&a, &b)) in before.iter().zip(after).enumerate() {
        if a != b {
            m.write_u16(base + i as u32 * 2, b)?;
        }
    }
    Ok(())
}
fn word(m: &mut dyn FrameMemory, a: u32, v: u32) -> Result<(), FrameError> {
    m.write_u16(a, v as u16)?;
    m.write_u16(a + 2, (v >> 16) as u16)
}
fn stack(m: &mut dyn FrameMemory, off: u32, args: &[u32]) -> Result<(), FrameError> {
    let base = if off >= 0x2f68 { AUX } else { MAIN };
    if matches!(off, 0x2ec0 | 0x2f68) {
        return m.write_u16(base, 0);
    }
    let before = read::<32>(m, base)?;
    let mut s = CallbackStack::from_halfwords(&before).map_err(|e| err(off, e))?;
    match (off, args) {
        (0x2ecc | 0x2f74, &[id, tag]) => {
            s.push(CallbackId(id), tag);
        }
        (0x2f00 | 0x2fa8, []) => {
            s.pop();
        }
        (0x2f14, &[id, tag]) => s.replace(CallbackId(id), tag),
        _ => return Err(err(off, "invalid callback arguments")),
    }
    write(m, base, &before, &s.to_halfwords())
}
fn input(m: &mut dyn FrameMemory, off: u32, args: &[u32]) -> Result<(), FrameError> {
    let before = read::<45>(m, INPUT)?;
    let mut s = InputState::from_halfwords(&before).map_err(|e| err(off, e))?;
    match (off, args) {
        (0x2c64, &[delay, min, accel]) => s.initialize(delay, min, accel),
        (0x2c9c, []) => s.poll_keyinput(m.read_u16(0x04000130)?),
        (0x2cc0, &[mask]) => s.repeat_grouped(mask as u16),
        (0x2dcc, &[mask]) => s.repeat_per_key(mask as u16),
        _ => return Err(err(off, "invalid input arguments")),
    }
    write(m, INPUT, &before, &s.to_halfwords())
}
/// Same 0x81E8 source behavior as startup::initial_callback, bound to its actual
/// native stacks. Dependency-side mutations are visible to every later load.
pub fn initial(
    m: &mut dyn FrameMemory,
    c: &mut dyn LifecycleCalls,
) -> Result<startup::StartupOutcome, FrameError> {
    let depth = m.read_u16(MAIN)?;
    if !(1..=10).contains(&depth) {
        return Err(err(0x81e8, "invalid main callback depth"));
    }
    let slot = MAIN + 44 + 2 * u32::from(depth - 1);
    let state = m.read_u16(slot)? as i16;
    if state == 0 || state == 1 {
        m.write_u16(slot, 2)?;
        word(m, startup::CONTEXT_POINTER_SLOT, startup::INITIAL_CONTEXT)?;
        c.call(
            m,
            if state == 0 { 0x6ef4 } else { 0x7840 },
            &[startup::INITIAL_CONTEXT],
        )?;
        return Ok(if state == 0 {
            startup::StartupOutcome::Initialized
        } else {
            startup::StartupOutcome::Resumed
        });
    }
    let context = m.read_u32(startup::CONTEXT_POINTER_SLOT)?;
    c.call(m, 0x8168, &[context])?;
    let context = m.read_u32(startup::CONTEXT_POINTER_SLOT)?;
    if m.read_u32(context)? != 0 {
        stack(m, 0x2f14, &[startup::NEXT_CALLBACK, 0])?;
        stack(m, 0x2f68, &[])?;
        Ok(startup::StartupOutcome::ReplacedByNextCallback)
    } else {
        Ok(startup::StartupOutcome::Updated)
    }
}
pub struct ControlCalls<'a>(pub &'a mut dyn LifecycleCalls);
impl LifecycleCalls for ControlCalls<'_> {
    fn call(&mut self, m: &mut dyn FrameMemory, off: u32, args: &[u32]) -> Result<(), FrameError> {
        match off {
            0x81e8 if args.is_empty() => initial(m, self.0).map(|_| ()),
            0x2ec0 | 0x2f68 if args.is_empty() => stack(m, off, args),
            0x2ecc | 0x2f00 | 0x2f14 | 0x2f74 | 0x2fa8 => stack(m, off, args),
            0x2c64 | 0x2c9c | 0x2cc0 | 0x2dcc => input(m, off, args),
            0x81e8 | 0x2ec0 | 0x2f68 => Err(err(off, "invalid callback arguments")),
            _ => self.0.call(m, off, args),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::memory::{MemoryImage, Region};
    use serde_json::{json, Value};
    fn n(v: &Value) -> u32 {
        v.as_u64().unwrap() as u32
    }
    fn words(v: &Value) -> Vec<u8> {
        v.as_array()
            .unwrap()
            .iter()
            .flat_map(|v| (n(v) as u16).to_le_bytes())
            .collect()
    }
    fn reg(base: u32, bytes: Vec<u8>) -> Region {
        Region {
            base,
            bytes,
            writable: true,
        }
    }
    struct Probe<'a> {
        case: &'a Value,
        calls: Vec<Value>,
    }
    impl LifecycleCalls for Probe<'_> {
        fn call(
            &mut self,
            m: &mut dyn FrameMemory,
            off: u32,
            args: &[u32],
        ) -> Result<(), FrameError> {
            self.calls
                .push(json!({"entry_offset":off,"argument0":args[0]}));
            if off == 0x8168 {
                word(
                    m,
                    startup::CONTEXT_POINTER_SLOT,
                    n(&self.case["returnPointer"]),
                )?;
                word(
                    m,
                    n(&self.case["returnPointer"]),
                    n(&self.case["returnCompletion"]),
                )?;
            }
            Ok(())
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_address_initial_wrapper_matches_all_original_stack_snapshots() {
        let f: Value = serde_json::from_str(vectors!("startup-vectors.json")).unwrap();
        for c in f["cases"].as_array().unwrap() {
            let mut m = MemoryImage::fixture(vec![
                reg(MAIN, words(&c["mainInitial"])),
                reg(AUX, words(&c["auxInitial"])),
                reg(
                    startup::CONTEXT_POINTER_SLOT,
                    n(&c["pointerInitial"]).to_le_bytes().to_vec(),
                ),
                reg(
                    0x02002000,
                    n(&c["completionInitial"]).to_le_bytes().to_vec(),
                ),
                reg(0x02003000, vec![0x55; 4]),
            ]);
            let mut probe = Probe {
                case: c,
                calls: Vec::new(),
            };
            initial(&mut m, &mut probe).unwrap();
            assert_eq!(m.bytes(MAIN, 64).unwrap(), words(&c["mainFinal"]));
            assert_eq!(m.bytes(AUX, 64).unwrap(), words(&c["auxFinal"]));
            assert_eq!(
                m.read_u32(startup::CONTEXT_POINTER_SLOT).unwrap(),
                n(&c["pointerFinal"])
            );
            assert_eq!(json!(probe.calls), c["calls"]);
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_address_input_adapter_matches_original_operation_sequences() {
        let f: Value = serde_json::from_str(vectors!("control-vectors.json")).unwrap();
        for c in f["inputCases"].as_array().unwrap() {
            let mut m = MemoryImage::fixture(vec![
                reg(INPUT, words(&c["initial"])),
                reg(0x04000130, vec![0; 2]),
            ]);
            for (op, snapshot) in c["operations"]
                .as_array()
                .unwrap()
                .iter()
                .zip(c["snapshots"].as_array().unwrap())
            {
                let (off, args) = match op["kind"].as_str().unwrap() {
                    "initialize" => (
                        0x2c64,
                        vec![
                            n(&op["initialDelay"]),
                            n(&op["minimumDelay"]),
                            n(&op["acceleration"]),
                        ],
                    ),
                    "poll" => {
                        m.write_u16(0x04000130, n(&op["raw"]) as u16).unwrap();
                        (0x2c9c, vec![])
                    }
                    "group" => (0x2cc0, vec![n(&op["mask"])]),
                    "per-key" => (0x2dcc, vec![n(&op["mask"])]),
                    _ => panic!("unknown fixture operation"),
                };
                input(&mut m, off, &args).unwrap();
                assert_eq!(m.bytes(INPUT, 90).unwrap(), words(snapshot));
            }
        }
    }
}
