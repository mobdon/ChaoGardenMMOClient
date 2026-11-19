"""Compile exact local pinned C functions as an independent runtime oracle.

No third-party C implementation is embedded here. Supply the ignored local
reference snapshot; its Git blob identities are verified before extraction.
The resulting C/executable/JSON must remain inside an explicit .local path.
Host stubs bind input registers, task storage and audio only. This is a test
oracle for Rust logical behavior, not a playable C port or GBA emulator.
"""

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

COMMIT = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
SOURCES = {
    "src_core.c": "467638473172572ced7c26b26f11e52b105d4858",
    "src_game_math.c": "a336e7d10c44d246a2ba1f7f2b8bd56ade19c758",
    "include_core.h": "7da347250fa488b8002227e59a1bf8522e1ade41",
    "include_global.h": "75ea7eee87fbd03c8ac2b0bc37845a710aadfdac",
    "src_task.c": "081acd785c6ca3f316ce9a202015a21e72b1491d",
    "include_task.h": "1b2ee9d35defc9831dacf97fafd2f71823e25df2",
}


def source_files(directory):
    result = {}
    for name, expected in SOURCES.items():
        data = (directory / name).read_bytes()
        actual = hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()
        if actual != expected:
            raise ValueError(f"Reference Git blob mismatch: {name}: {actual}")
        result[name] = data.decode("utf-8")
    return result


def function(text, name):
    # Selected pinned functions contain no braces inside string literals.
    found = re.search(r"(?m)^(?:void|u16)\s+" + re.escape(name) + r"\([^;]*?\)\s*\{", text)
    if not found:
        raise ValueError(f"Cannot locate function definition {name}")
    depth = 1
    end = found.end()
    while depth:
        if end >= len(text):
            raise ValueError(f"Unbalanced function {name}")
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[found.start():end]


def macro(text, name):
    found = re.search(r"(?m)^#define\s+" + re.escape(name) + r"\([^\n]*", text)
    if not found:
        raise ValueError(f"Cannot locate macro {name}")
    end = found.end()
    while text[:end].rstrip().endswith("\\"):
        end = text.find("\n", end + 1)
        if end == -1:
            raise ValueError(f"Unterminated macro {name}")
    return text[found.start():end]


def no_includes(text):
    return re.sub(r"(?m)^#include[^\n]*", "", text)


PREAMBLE = r'''
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <assert.h>
typedef uint8_t u8; typedef int8_t s8; typedef uint16_t u16;
typedef int16_t s16; typedef uint32_t u32; typedef int32_t s32;
#define TRUE 1
#define FALSE 0
#define PORTABLE 1
#define PLATFORM_GBA 0
#define DEBUG 0
#define TAS_TESTING 0
#define GAME 2
#define GAME_SA2 2
#define ENGINE 2
#define ENGINE_1 1
#define ENGINE_2 2
#define ENGINE_3 3
#define BUG_FIX 1
#define NON_MATCHING 1
#define UNUSED __attribute__((unused))
#define ALIGNED(n) __attribute__((aligned(n)))
#define FLAGS_800 0x800
#define SA2_LABEL(x) x
#define GetBit(x,y) (((x) >> (y)) & 1)
#define DmaFill32(channel,value,target,length) memset((target),0,(length))
#define DmaCopy32(channel,source,target,length) memcpy((target),(source),(length))
static u16 REG_KEYINPUT;
#define KEYS_MASK 0x3FF
u16 gPhysicalInput, gInput, gPrevInput, gPressedKeys, gReleasedKeys, gRepeatedKeys;
u8 gRepeatedKeysTestCounter[10], gKeysFirstRepeatIntervals[10], gKeysContinuedRepeatIntervals[10];
struct { int mode; } gInputRecorder;
#define RECORDER_RECORD 1
#define RECORDER_PLAYBACK 2
static u16 playback_word;
void InputRecorderWrite(u16 word) {(void)word;}
u16 InputRecorderRead(void) { return playback_word; }
u32 gRngPrevValue, gRngValue;
s32 gPseudoRandom;
#define RAND_CONST 0x37119371;
'''

TASK_GLOBALS = r'''
Task gTasks[MAX_TASK_NUM], gEmptyTask;
Task *gTaskPtrs[MAX_TASK_NUM], *gNextTask, *gCurTask;
s32 gNumTasks;
u8 gIwramHeap[TASK_HEAP_SIZE];
void *iwram_end;
u32 gFlags;
int gExecSoundMain;
void m4aSoundMain(void) {}
'''

HARNESS = r'''
static void print_u16_array(u8 *array, int length) {
    printf("["); for(int i=0;i<length;i++) printf("%s%u",i?",":"",array[i]); printf("]");
}
static int events[1024], event_count, mutation_mode;
static Task *future;
struct TestData { int label; int ran; };
static void event(int value) { assert(event_count < 1024); events[event_count++] = value; }
static void destructor(Task *task) { struct TestData *data = TASK_DATA(task); event(200+data->label); }
static Task *make(int label, u16 priority, u16 flags);
static void task_main(void) {
    struct TestData *data = TASK_DATA(gCurTask);
    event(100+data->label);
    if(mutation_mode && data->label==1 && !data->ran) {
        data->ran=1; make(3,15,0); future=make(4,30,0);
    } else if(mutation_mode && data->label==2 && future) {
        TaskDestroy(future); future=NULL;
    } else if(mutation_mode && data->label==5) {
        TaskDestroy(gCurTask);
    }
}
static Task *make(int label,u16 priority,u16 flags) {
    Task *task=TaskCreate(task_main,sizeof(struct TestData),priority,flags,destructor);
    assert(task!=&gEmptyTask); ((struct TestData *)TASK_DATA(task))->label=label; return task;
}
static void print_events(void) {
    printf("[");for(int i=0;i<event_count;i++)printf("%s%d",i?",":"",events[i]);printf("]");
}
int main(void) {
    assert(sizeof(u32)==4); u32 endian=1; assert(*(u8 *)&endian==1);
    memset(gKeysFirstRepeatIntervals,20,10);memset(gKeysContinuedRepeatIntervals,8,10);
    printf("{\"input\":[");
    for(int frame=0;frame<72;frame++) {
        u16 held=frame<36?1:(frame<40?0:(frame<60?0x221:0x082));
        if(frame==36)held=0;
        int playback=frame>=50&&frame<55;
        playback_word=(frame&1)?0x8002:0x8301;
        REG_KEYINPUT=(u16)~held;gInputRecorder.mode=playback?RECORDER_PLAYBACK:0;
        GetInput();
        printf("%s{\"held\":%u,\"playback\":",frame?",":"",held);
        if(playback)printf("%u",playback_word);else printf("null");
        printf(",\"current\":%u,\"physical\":%u,\"pressed\":%u,\"released\":%u,\"repeated\":%u,\"counters\":",gInput,gPhysicalInput,gPressedKeys,gReleasedKeys,gRepeatedKeys);
        print_u16_array(gRepeatedKeysTestCounter,10);printf("}");
    }
    printf("],\"rng\":[");
    const u32 seeds[][2]={{0,0},{UINT32_MAX,UINT32_MAX},{0x12345678,0x9ABCDEF0}};
    for(int seed=0;seed<3;seed++) {
        SeedRng(seeds[seed][0],seeds[seed][1]);
        printf("%s{\"previous\":%u,\"current\":%u,\"outputs\":[",seed?",":"",seeds[seed][0],seeds[seed][1]);
        for(int step=0;step<64;step++) { u16 value=Random();printf("%s[%u,%u,%u]",step?",":"",value,gRngPrevValue,gRngValue); }
        printf("]}");
    }
    printf("],\"pseudo\":[");
    const u32 pseudo_seeds[]={0,UINT32_MAX,0x80000000};
    for(int seed=0;seed<3;seed++) {
        gPseudoRandom=(s32)pseudo_seeds[seed];printf("%s{\"seed\":%u,\"outputs\":[",seed?",":"",pseudo_seeds[seed]);
        for(int step=0;step<64;step++) { u32 value=PSEUDO_RANDOM_32();printf("%s%u",step?",":"",value); }printf("]}");
    }
    printf("],\"fixed\":[");
    const s32 values[][2]={{0,1},{1,256},{-1,256},{-257,129},{0x7FFFFFFF,0x10001},{(s32)0x80000000,257},{1234567,-32767}};
    for(int i=0;i<7;i++) { s32 a=values[i][0],b=values[i][1];
        printf("%s{\"a\":%d,\"b\":%d,\"fromInt\":%d,\"integer\":%d,\"fraction\":%u,\"product\":%d,\"negativeProduct\":%d,\"quotient\":%d}",i?",":"",a,b,Q_24_8(a),Q_24_8_TO_INT(a),Q_24_8_FRAC(a),Q_MUL(a,b),Q_MUL_NEG(a,b),Q_DIV2(a,b));
    }
    printf("],\"tasks\":{\"static\":");
    TasksInit();make(1,100,0);make(2,100,0);make(3,50,4);make(4,150,1);Task *protected=make(5,200,2);
    gFlags=0;TasksExec();gFlags=0x800;TasksExec();TasksDestroyInPriorityRange(100,200);TaskDestroy(protected);print_events();
    printf(",\"staticCount\":%d,\"mutation\":",gNumTasks);
    event_count=0;mutation_mode=1;gFlags=0;TasksInit();make(1,10,0);make(2,20,0);make(5,40,0);TasksExec();TasksExec();print_events();
    printf(",\"mutationCount\":%d}}",gNumTasks);return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--zig", type=pathlib.Path, required=True)
    parser.add_argument("--summary", type=pathlib.Path,
                        help="Optional public metadata-only summary; contains hashes/counts, no C or state vectors")
    args = parser.parse_args()
    output = args.output_dir.resolve()
    if ".local" not in output.parts:
        parser.error("Oracle outputs must remain in an explicit .local directory")
    files = source_files(args.reference_dir)
    body = [PREAMBLE]
    body.extend(function(files["src_core.c"], name) for name in ["GetInput"])
    body.extend(function(files["src_game_math.c"], name) for name in ["Random", "SeedRng"])
    body.append(macro(files["include_core.h"], "PSEUDO_RANDOM_32"))
    body.extend(macro(files["include_global.h"], name) for name in [
        "Q_24_8", "Q_24_8_TO_INT", "Q_24_8_FRAC", "Q_MUL", "Q_MUL_NEG", "Q_DIV2"])
    body.extend([no_includes(files["include_task.h"]), TASK_GLOBALS,
                 no_includes(files["src_task.c"]), HARNESS])
    output.mkdir(parents=True, exist_ok=True)
    c_path = output / "runtime-oracle.c"
    executable = output / ("runtime-oracle.exe" if sys.platform == "win32" else "runtime-oracle")
    c_path.write_text("\n".join(body), encoding="utf-8")
    subprocess.run([str(args.zig), "cc", "-std=gnu11", "-O1", "-fwrapv",
                    "-fno-strict-aliasing", str(c_path), "-o", str(executable)], check=True)
    report = json.loads(subprocess.check_output([str(executable)], text=True))
    report.update(referenceCommit=COMMIT, sourceGitBlobs=SOURCES,
                  scope="native C logical behavior with host register/storage stubs; no GBA heap or interrupt parity")
    target = output / "runtime-oracle.json"
    target.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if args.summary:
        canonical = json.dumps(report, sort_keys=True, separators=(",", ":")).encode()
        summary = {
            "schemaVersion": 1,
            "referenceCommit": COMMIT,
            "sourceGitBlobs": SOURCES,
            "oracleCanonicalSha256": hashlib.sha256(canonical).hexdigest(),
            "inputFrames": len(report["input"]),
            "mathRngOutputs": sum(len(item["outputs"]) for item in report["rng"]),
            "lcgOutputs": sum(len(item["outputs"]) for item in report["pseudo"]),
            "fixedPointCases": len(report["fixed"]),
            "taskCallbackEvents": len(report["tasks"]["static"]) + len(report["tasks"]["mutation"]),
            "scope": report["scope"],
        }
        args.summary.parent.mkdir(parents=True, exist_ok=True)
        args.summary.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(target), "inputFrames": len(report["input"]),
                      "rngOutputs": sum(len(item["outputs"]) for item in report["rng"]),
                      "pseudoOutputs": sum(len(item["outputs"]) for item in report["pseudo"]),
                      "fixedCases": len(report["fixed"]), "taskEvents": report["tasks"]}))


if __name__ == "__main__":
    main()
