//! Native A3VE staged-OAM frame initialization, queue resets and DMA request.
//! Hardware commands preserve original register order. Returning a command is
//! not evidence that a host applied DMA or reproduced GBA transfer timing.

use super::oam_render::{StagedOam, OUTPUT_BASE};
use serde::Serialize;

pub const AFFINE_COUNT_ADDRESS: u32 = 0x0300_005c;
pub const HIDE_OFFSET: usize = 0x1ddc;
pub const AFFINE_OFFSET: usize = 0x1e28;
pub const BEGIN_OFFSET: usize = 0x1dc4;
pub const DMA_OFFSET: usize = 0x2560;
pub const RESET_QUEUE_OFFSET: usize = 0x2580;
pub const RESET_TREE_OFFSET: usize = 0x28ec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsFrameState {
    pub staged: StagedOam,
    pub affine_count: u8,
    pub submission_count: u8,
    pub tree_root: u32,
}

impl GraphicsFrameState {
    /// 0x1ddc: all 128 objects receive native off-screen attr0/attr1 values.
    /// Attr2 and affine/reserved halfwords are preserved; output count is zero.
    pub fn hide_objects(&mut self) {
        for entry in &mut self.staged.entries {
            entry[0] = 0xc0;
            entry[1] = 0x160;
        }
        self.staged.count = 0;
    }

    /// 0x1e28: initialize exactly the first eight affine matrices (32 fourth
    /// halfwords). Later affine fields retain their prior contents.
    pub fn initialize_affine(&mut self, argument: u32) {
        for (index, entry) in self.staged.entries[..32].iter_mut().enumerate() {
            entry[3] = if index % 4 == 0 || index % 4 == 3 {
                0x100
            } else {
                0
            };
        }
        self.affine_count = argument as u8;
    }

    /// 0x1dc4 calls both actual native helpers in this order.
    pub fn begin_frame(&mut self, argument: u32) {
        self.hide_objects();
        self.initialize_affine(argument);
    }

    /// 0x2580 does not clear any queue record/order storage or the tree root.
    pub fn reset_submission_count(&mut self) {
        self.submission_count = 0;
    }

    /// 0x28ec clears the submission count and tree-root pointer only.
    pub fn reset_tree(&mut self) {
        self.submission_count = 0;
        self.tree_root = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HardwareCommand {
    Write32 { address: u32, value: u32 },
    Read32 { address: u32 },
}

/// 0x2560's exact DMA3 register transaction, including the final control read.
/// No DMA register auto-clear behavior, memory transfer or timing is invented.
pub fn oam_dma_commands() -> [HardwareCommand; 4] {
    [
        HardwareCommand::Write32 {
            address: 0x0400_00d4,
            value: OUTPUT_BASE,
        },
        HardwareCommand::Write32 {
            address: 0x0400_00d8,
            value: 0x0700_0000,
        },
        HardwareCommand::Write32 {
            address: 0x0400_00dc,
            value: 0x8400_0100,
        },
        HardwareCommand::Read32 {
            address: 0x0400_00dc,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn integer(value: &Value) -> u32 {
        value.as_u64().unwrap() as u32
    }
    fn pattern(seed: u32) -> [[u16; 4]; 128] {
        let words: Vec<_> = (0..256)
            .map(|index| seed.wrapping_add((index as u32).wrapping_mul(0x0102_0304)))
            .collect();
        std::array::from_fn(|index| {
            [
                words[index * 2] as u16,
                (words[index * 2] >> 16) as u16,
                words[index * 2 + 1] as u16,
                (words[index * 2 + 1] >> 16) as u16,
            ]
        })
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_frame_helpers_match_complete_staged_ram_and_scalar_state() {
        let data: Value = serde_json::from_str(vectors!("graphics_frame-vectors.json")).unwrap();
        for case in data["cases"].as_array().unwrap() {
            let mut state = GraphicsFrameState {
                staged: StagedOam {
                    count: integer(&case["initialOutputCount"]) as u8,
                    entries: pattern(integer(&case["seed"])),
                },
                affine_count: integer(&case["initialAffineCount"]) as u8,
                submission_count: integer(&case["initialSubmissionCount"]) as u8,
                tree_root: integer(&case["initialRoot"]),
            };
            for step in case["steps"].as_array().unwrap() {
                match step["operation"].as_str().unwrap() {
                    "hide" => state.hide_objects(),
                    "affine" => state.initialize_affine(integer(&step["argument"])),
                    "begin" => state.begin_frame(integer(&step["argument"])),
                    "queue" => state.reset_submission_count(),
                    "tree" => state.reset_tree(),
                    _ => panic!("unknown fixture operation"),
                }
                assert_eq!(u32::from(state.staged.count), integer(&step["outputCount"]));
                assert_eq!(u32::from(state.affine_count), integer(&step["affineCount"]));
                assert_eq!(
                    u32::from(state.submission_count),
                    integer(&step["submissionCount"])
                );
                assert_eq!(state.tree_root, integer(&step["root"]));
                let bytes: Vec<_> = state
                    .staged
                    .entries
                    .into_iter()
                    .flatten()
                    .flat_map(u16::to_le_bytes)
                    .collect();
                assert_eq!(
                    crate::sha256(&bytes),
                    step["hardwareSha256"].as_str().unwrap()
                );
            }
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_dma_register_write_read_order_matches_typed_request() {
        let data: Value = serde_json::from_str(vectors!("graphics_frame-vectors.json")).unwrap();
        assert_eq!(
            serde_json::to_value(oam_dma_commands()).unwrap(),
            data["dmaCommands"]
        );
    }
}
