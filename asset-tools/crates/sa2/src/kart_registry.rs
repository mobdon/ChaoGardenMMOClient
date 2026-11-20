//! Vehicle-task membership check used by CartD pickup callbacks at +0x1714.
//! Zero is the native null pointer. Empty slots do not terminate the scan.
pub fn slot(tasks: &[u32; 6], contact: u32) -> Option<usize> {
    if contact == 0 {
        return None;
    }
    tasks.iter().position(|&task| task == contact)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn holes_null_and_duplicate_task_pointers() {
        let tasks = [0, 0xff01, 0, 0xff01, 0, 0xff05];
        for (task, expected) in [
            (0, None),
            (0xff02, None),
            (0xff01, Some(1)),
            (0xff05, Some(5)),
        ] {
            assert_eq!(slot(&tasks, task), expected);
        }
    }
}
