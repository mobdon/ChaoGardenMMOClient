//! Opt-in executable comparison with exact pinned C functions. The oracle's
//! hardware/storage stubs and generated source stay under ignored .local.

use serde::Deserialize;
use sonic_advance_2_assets::native_runtime::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    reference_commit: String,
    input: Vec<InputCase>,
    rng: Vec<RngCase>,
    pseudo: Vec<PseudoCase>,
    fixed: Vec<FixedCase>,
    tasks: TasksCase,
}

#[derive(Deserialize)]
struct InputCase {
    held: u16,
    playback: Option<u16>,
    #[serde(flatten)]
    frame: InputFrame,
}

#[derive(Deserialize)]
struct RngCase {
    previous: u32,
    current: u32,
    outputs: Vec<[u32; 3]>,
}

#[derive(Deserialize)]
struct PseudoCase {
    seed: u32,
    outputs: Vec<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedCase {
    a: i32,
    b: i32,
    from_int: i32,
    integer: i32,
    fraction: u8,
    product: i32,
    negative_product: i32,
    quotient: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TasksCase {
    r#static: Vec<u32>,
    static_count: usize,
    mutation: Vec<u32>,
    mutation_count: usize,
}

#[derive(Default)]
struct Trace {
    mutation: bool,
    future: Option<TaskId>,
    events: Vec<u32>,
}

struct ExampleData {
    label: u32,
    ran: bool,
    trace: Rc<RefCell<Trace>>,
}

fn create(
    scheduler: &mut TaskScheduler<ExampleData>,
    label: u32,
    priority: u16,
    flags: u16,
    trace: Rc<RefCell<Trace>>,
) -> TaskId {
    scheduler
        .create(
            priority,
            flags,
            ExampleData {
                label,
                ran: false,
                trace,
            },
            main_callback,
            Some(destructor),
        )
        .unwrap()
}

fn destructor(scheduler: &mut TaskScheduler<ExampleData>, id: TaskId) {
    let data = scheduler.data(id).unwrap();
    data.trace.borrow_mut().events.push(200 + data.label);
}

fn main_callback(scheduler: &mut TaskScheduler<ExampleData>, id: TaskId) {
    let (label, ran, trace) = {
        let data = scheduler.data(id).unwrap();
        (data.label, data.ran, data.trace.clone())
    };
    trace.borrow_mut().events.push(100 + label);
    let mutation = trace.borrow().mutation;
    if mutation && label == 1 && !ran {
        scheduler.data_mut(id).unwrap().ran = true;
        create(scheduler, 3, 15, 0, trace.clone());
        let future = create(scheduler, 4, 30, 0, trace.clone());
        trace.borrow_mut().future = Some(future);
    } else if mutation && label == 2 {
        let future = trace.borrow_mut().future.take();
        if let Some(future) = future {
            scheduler.destroy(future).unwrap();
        }
    } else if mutation && label == 5 {
        scheduler.destroy(id).unwrap();
    }
}

#[test]
#[ignore = "Run native_runtime_oracle.py locally and set A2NE_RUNTIME_ORACLE to its JSON output"]
fn native_c_input_rng_arithmetic_and_task_execution_match_rust() {
    let path = std::env::var_os("A2NE_RUNTIME_ORACLE").expect("Set A2NE_RUNTIME_ORACLE");
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let summary: serde_json::Value =
        serde_json::from_str(include_str!("../catalog/native-runtime-parity.json")).unwrap();
    assert_eq!(
        sonic_advance_2_assets::gba::sha256(&serde_json::to_vec(&value).unwrap()),
        summary["oracleCanonicalSha256"].as_str().unwrap(),
        "exact compiled-native C oracle metadata identity"
    );
    let oracle: Oracle = serde_json::from_value(value).unwrap();
    assert_eq!(
        oracle.reference_commit,
        sonic_advance_2_assets::REFERENCE_COMMIT
    );
    assert_eq!(oracle.input.len(), 72);
    let mut input = InputState::default();
    for (frame, expected) in oracle.input.iter().enumerate() {
        assert_eq!(
            input.poll_keyinput(!expected.held, expected.playback),
            expected.frame,
            "native GetInput frame {frame}"
        );
    }
    assert_eq!(oracle.rng.len(), 3);
    for case in &oracle.rng {
        assert_eq!(case.outputs.len(), 64);
        let mut rng = NativeRng::seeded(case.previous, case.current);
        for expected in &case.outputs {
            let value = rng.next_u16();
            assert_eq!([u32::from(value), rng.previous, rng.current], *expected);
        }
    }
    assert_eq!(oracle.pseudo.len(), 3);
    for case in &oracle.pseudo {
        assert_eq!(case.outputs.len(), 64);
        let mut rng = PseudoRng::seeded(case.seed);
        for expected in &case.outputs {
            assert_eq!(rng.next_u32(), *expected);
        }
    }
    assert_eq!(oracle.fixed.len(), 7);
    for case in &oracle.fixed {
        assert_eq!(q24_8_from_int(case.a), case.from_int);
        assert_eq!(q24_8_to_int(case.a), case.integer);
        assert_eq!(q24_8_fraction(case.a), case.fraction);
        assert_eq!(q24_8_mul(case.a, case.b), case.product);
        assert_eq!(q24_8_mul_neg(case.a, case.b), case.negative_product);
        assert_eq!(q24_8_div(case.a, case.b).unwrap(), case.quotient);
    }

    let mut scheduler = TaskScheduler::default();
    let trace = Rc::new(RefCell::new(Trace::default()));
    create(&mut scheduler, 1, 100, 0, trace.clone());
    create(&mut scheduler, 2, 100, 0, trace.clone());
    create(
        &mut scheduler,
        3,
        50,
        TASK_RUN_DURING_FLAGS_800,
        trace.clone(),
    );
    create(&mut scheduler, 4, 150, TASK_INACTIVE, trace.clone());
    let protected = create(&mut scheduler, 5, 200, TASK_DESTROY_DISABLED, trace.clone());
    scheduler.execute(0, 128).unwrap();
    scheduler.execute(FLAGS_800, 128).unwrap();
    scheduler.destroy_in_priority_range(100, 200).unwrap();
    assert!(!scheduler.destroy(protected).unwrap());
    assert_eq!(trace.borrow().events, oracle.tasks.r#static);
    assert_eq!(scheduler.game_task_count() + 2, oracle.tasks.static_count);

    let mut scheduler = TaskScheduler::default();
    let trace = Rc::new(RefCell::new(Trace {
        mutation: true,
        ..Trace::default()
    }));
    create(&mut scheduler, 1, 10, 0, trace.clone());
    create(&mut scheduler, 2, 20, 0, trace.clone());
    create(&mut scheduler, 5, 40, 0, trace.clone());
    scheduler.execute(0, 128).unwrap();
    scheduler.execute(0, 128).unwrap();
    assert_eq!(trace.borrow().events, oracle.tasks.mutation);
    assert_eq!(scheduler.game_task_count() + 2, oracle.tasks.mutation_count);
}
