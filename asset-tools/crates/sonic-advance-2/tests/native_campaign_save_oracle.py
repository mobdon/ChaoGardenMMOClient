"""Compile exact pinned campaign-save C against a bounded in-memory flash stub.

All profiles, original sources, executables and vectors stay in .local. Only
counts, source hashes and output fingerprints are suitable for publication.
This is C/lifecycle parity, not physical flash or real-save compatibility proof.
"""

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

PIN = "2dd3f17d30cd3c156e508aac0017620ae485b9ba"
SOURCES = {
    "src/game/sa2/save.c": "7bdb3cc5afb1fef5fc4e555c8ea2f5911bc6db82",
    "include/game/sa2/save.h": "72ae6b741aa9cf50a01b8ce7cc30ffb78517fa6c",
    "include/game/sa2/stage/player_controls.h": "09375b3f7d2b25c2adf7f96d58b5372ba6946d46",
    "include/game/globals.h": "dff72b844fae51ba1af3fa7f8040e17ec88f8833",
    "include/game/math.h": "574b4e7d050e6c98eb9b38fbb7e79aa3cd586e92",
    "include/constants/zones.h": "8743af77f3cf53d8c30127dbbdb540f596b61902",
    "include/constants/sa2/characters.h": "31139cd7efc1a05f9bbde67bf104e14524b66fbf",
    "include/constants/sa2/text.h": "8572d8108d32df8e948c80d510e4f3b4759a01df",
}


def function(source, name):
    pattern = r"(?m)^(?:static\s+)?(?:void|bool16|bool32|s16)\s+" + re.escape(name) + r"\([^;]*?\)\s*\{"
    found = re.search(pattern, source)
    if found is None:
        raise ValueError(f"Missing exact source function {name}")
    end, depth = found.end(), 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[found.start():end]


PREAMBLE = r'''
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
typedef uint8_t u8; typedef int8_t s8; typedef uint16_t u16; typedef int16_t s16;
typedef uint32_t u32; typedef int32_t s32; typedef u8 bool8; typedef u16 bool16; typedef u32 bool32;
#define TRUE 1
#define FALSE 0
#define GAME 2
#define GAME_SA1 1
#define GAME_SA2 2
#define GAME_SA3 3
#define GBA_FRAMES_PER_SECOND 60
#define TAS_TESTING 0
#define FLAGS_NO_FLASH_MEMORY 1
#define A_BUTTON 1
#define B_BUTTON 2
#define R_BUTTON 256
// Native uintptr_t is a 32-bit word. Host pointer width must not change sums.
#define uintptr_t uint32_t
'''

STUBS = r'''
_Static_assert(sizeof(struct SaveGame)==0x374,"SaveGame size");
_Static_assert(offsetof(struct SaveGame,timeRecords)==0x32,"in-memory record offset");
_Static_assert(offsetof(struct SaveGame,multiplayerScores)==0x2a8,"in-memory score table offset");
_Static_assert(sizeof(struct MultiplayerScore)==20,"MultiplayerScore size");
_Static_assert(sizeof(struct SaveSectorData)==0x378,"SaveSectorData size");
_Static_assert(offsetof(struct SaveSectorData,checksum)==0x374,"checksum offset");
_Static_assert(NUM_TIME_RECORD_ROWS==315,"time-record count");
struct SaveGame currentGame,lastGame;
struct SaveSectorData sectorBuffer;
struct SaveGame *gLoadedSaveGame=&currentGame,*gLastWrittenSaveGame=&lastGame;
struct SaveSectorData *gSaveSectorDataBuffer=&sectorBuffer;
u32 gFlags,randomCalls;
static u8 flash[16*4096];
static u16 programError,eraseErrors[10];
static s32 programmedSector,lastPayloadRead;
static u32 eraseAttempts,eraseSuccesses;
static u32 next(u32 x){x^=x<<13;x^=x>>17;return x^(x<<5);}
u16 Random(void){u32 n=++randomCalls;return (n*0x9e37)^(n>>3);}
void ReadFlash(s16 sector,u32 offset,void*target,u32 size){
 if(sector<0||sector>=10||offset+size>4096)exit(6);
 memcpy(target,flash+sector*4096+offset,size);
 if(size==sizeof(struct SaveSectorData))lastPayloadRead=sector;
}
static u16 EraseSaveSector(s16 sector){
 if(sector<0||sector>=10)exit(7);eraseAttempts|=1u<<sector;
 if(eraseErrors[sector])return eraseErrors[sector];
 memset(flash+sector*4096,255,4096);eraseSuccesses|=1u<<sector;return 0;
}
static u16 WriteToSaveSector(struct SaveSectorData*data,s16 sector){
 if(sector<0||sector>=10)exit(8);programmedSector=sector;
 if(programError)return programError;
 memset(flash+sector*4096,255,4096);
 memcpy(flash+sector*4096,data,sizeof(*data));return 0;
}
static void put32(u8*raw,int offset,u32 value){memcpy(raw+offset,&value,4);}
'''

DRIVER = r'''
static FILE*output(const char*root,const char*name){char path[4096];snprintf(path,sizeof(path),"%s/%s",root,name);FILE*f=fopen(path,"wb");if(!f)exit(4);return f;}
static void bytes(FILE*f,const void*p,size_t n){if(fwrite(p,1,n,f)!=n)exit(5);}
static void profiles(const char*root){
 FILE*f=output(root,"profiles.bin");u32 seed=0xa2ca5e01;
 struct SaveGame input,unpacked,completed;
 struct SaveSectorData packed;
 GenerateNewSaveGame(&input);InitSaveGameSectorData(&packed);
 bytes(f,&input,sizeof(input));bytes(f,&packed,sizeof(packed));
 for(u32 row=0;row<256;row++){
  for(u32 j=0;j<sizeof(input);j++){seed=next(seed);((u8*)&input)[j]=seed>>24;}
  if(row%16==0)input.id=0;
  if(row%4==0){input.buttonConfig.jump=1;input.buttonConfig.attack=2;input.buttonConfig.trick=256;}
  if(row%4==1){input.buttonConfig.jump=256;input.buttonConfig.attack=1;input.buttonConfig.trick=2;}
  if(row%4==2){input.buttonConfig.jump=0;input.buttonConfig.attack=3;input.buttonConfig.trick=257;}
  seed=next(seed);memset(&packed,0,sizeof(packed));packed.header.version=seed;
  randomCalls=row*4;PackSaveSectorData(&packed,&input);UnpackSaveSectorData(&unpacked,&packed);
  completed=input;GenerateCompletedSaveGame(&completed);
  bytes(f,&packed,sizeof(packed));bytes(f,&unpacked,sizeof(unpacked));bytes(f,&completed,sizeof(completed));bytes(f,&randomCalls,4);
 }fclose(f);
}
static void headers(const char*root){
 FILE*f=output(root,"headers.bin");u32 seed=0xa2ca5e02;
 u32 special[]={0,1,0xffff,0x10000,0xffff0000,0xfffffffe,0xffffffff};
 for(u32 row=0;row<1024;row++){
  memset(flash,0xa5,sizeof(flash));gFlags=0;
  for(int sector=0;sector<10;sector++){
   seed=next(seed);put32(flash,sector*4096,(seed&3)?SECTOR_SECURITY_NUM:seed);
   seed=next(seed);u32 version=(seed%8==7)?seed:special[seed%8];put32(flash,sector*4096+4,version);
  }
  s32 result[3]={FindNewestSaveGameSector(),FindOldestSaveGameSector(),SaveGameExists()};bytes(f,result,sizeof(result));
 }fclose(f);
}
static void fixture(int scenario){
 memset(flash,scenario==2?0:255,sizeof(flash));memset(flash+10*4096,0xa5,6*4096);
 memset(eraseErrors,0,sizeof(eraseErrors));programError=0;gFlags=(scenario==4)?FLAGS_NO_FLASH_MEMORY:0;randomCalls=0;
 if(scenario==1||scenario==3||scenario==7){
  int count=scenario==1?3:10;
  for(int sector=0;sector<count;sector++){
   struct SaveGame profile;struct SaveSectorData data;GenerateNewSaveGame(&profile);memset(&data,0,sizeof(data));
   profile.id=0x100+sector;profile.score=(sector+1)*111;data.header.version=sector;PackSaveSectorData(&data,&profile);
   if(scenario==3){data.header.version=sector==0?0xffffffff:(sector==1?0:sector);data.checksum=CalcChecksum(&data);}
   memcpy(flash+sector*4096,&data,sizeof(data));
   if(scenario==7)flash[sector*4096+80]^=1;
  }
  if(scenario==1)flash[2*4096+80]^=1;
 }
 GenerateNewSaveGame(&currentGame);GenerateNewSaveGame(&lastGame);InitSaveGameSectorData(&sectorBuffer);
 currentGame.language=3+scenario;currentGame.score=900+scenario;randomCalls=0;
 if(scenario==5)programError=7;
 if(scenario==6)eraseErrors[2]=8;
}
static void sessions(const char*root){
 FILE*f=output(root,"sessions.bin");u16 name[6]={11,22,33,44,55,0xffff};
 for(int scenario=0;scenario<8;scenario++){
  fixture(scenario);
  for(int step=0;step<8;step++){
   programmedSector=lastPayloadRead=-1;eraseAttempts=eraseSuccesses=0;s32 result=0;
   switch(step){
    case 0:case 6:result=TryLoadLatestSaveGame();break;
    case 1:currentGame.score+=17;currentGame.multiplayerWins=98;memcpy(currentGame.playerName,name,12);result=WriteSaveGame();break;
    case 2:result=WriteSaveGame();break;
    case 3:currentGame.id=0;currentGame.language=6;currentGame.score=1234;result=CreateAndTryWriteNewSaveGame();break;
    case 4:result=WriteSaveGame();break;
    case 5:GenerateCompletedSaveGame(&currentGame);result=WriteSaveGame();break;
    case 7:InsertMultiplayerProfile(42,name);RecordOwnMultiplayerResult(0);RecordMultiplayerResult(42,name,1);result=WriteSaveGame();break;
   }
   s32 row[10]={result,programmedSector,eraseAttempts,eraseSuccesses,randomCalls,HasChangesToSave(),SaveGameExists(),FindNewestSaveGameSector(),FindOldestSaveGameSector(),((step==0||step==6)&&result)?lastPayloadRead:-1};
   bytes(f,row,sizeof(row));bytes(f,&currentGame,sizeof(currentGame));bytes(f,&lastGame,sizeof(lastGame));bytes(f,&sectorBuffer,sizeof(sectorBuffer));bytes(f,flash,sizeof(flash));
  }
 }fclose(f);
}
int main(int argc,char**argv){if(argc!=2)return 2;u32 endian=1;if(*(u8*)&endian!=1)return 3;profiles(argv[1]);headers(argv[1]);sessions(argv[1]);return 0;}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--summary", type=pathlib.Path, required=True)
    parser.add_argument("--zig", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if sys.byteorder != "little" or ".local" not in args.output_dir.resolve().parts:
        parser.error("Oracle requires a little-endian host and ignored .local output")
    sources = {}
    for name, expected in SOURCES.items():
        raw = (args.reference_dir / name).read_bytes()
        if hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest() != expected:
            raise ValueError(f"Wrong exact pinned source {name}")
        sources[name] = raw.decode("utf-8")
    source = sources["src/game/sa2/save.c"]
    strip_includes = lambda text: re.sub(r"(?m)^#include[^\n]*", "", text)
    headers = [sources["include/constants/zones.h"], sources["include/constants/sa2/characters.h"],
        sources["include/constants/sa2/text.h"], re.search(r"(?m)^#define MAX_PLAYER_NAME_LENGTH .*", sources["include/game/globals.h"])[0],
        strip_includes(sources["include/game/sa2/stage/player_controls.h"]), strip_includes(sources["include/game/sa2/save.h"])]
    structs = source[source.index("struct SaveSectorHeader {"):source.index("struct SaveGame *gLastWrittenSaveGame")]
    macros = source[source.index("#define CalcChecksum"):source.index("void InsertMultiplayerProfile")]
    random_macro = re.search(r"(?m)^#define Random32\(\).*", sources["include/game/math.h"])[0]
    names = ["GenerateNewSaveGame", "InitSaveGameSectorData", "PackSaveSectorData", "UnpackSaveSectorData",
        "FindNewestSaveGameSector", "FindOldestSaveGameSector", "StringEquals", "InsertMultiplayerProfile",
        "RecordOwnMultiplayerResult", "RecordMultiplayerResult", "GenerateCompletedSaveGame", "HasChangesToSave",
        "ReadSaveSectorAndVerifyChecksum", "TryWriteSaveGame", "TryLoadLatestSaveGame", "CreateAndTryWriteNewSaveGame",
        "SaveGameExists", "WriteSaveGame"]
    functions = [function(source, name) for name in names]
    prototypes = "\n".join(text[:text.index("{")].strip() + ";" for text in functions)
    generated = "\n".join([PREAMBLE, *headers, structs, macros, random_macro, STUBS, prototypes, *functions, DRIVER])
    args.output_dir.mkdir(parents=True, exist_ok=True)
    c_file, executable = args.output_dir / "campaign-save.c", args.output_dir / "campaign-save.exe"
    c_file.write_text(generated)
    subprocess.run([str(args.zig), "cc", "-std=gnu11", "-O1", "-fwrapv", "-fno-strict-aliasing", str(c_file), "-o", str(executable)], check=True)
    subprocess.run([str(executable), str(args.output_dir)], check=True)
    outputs = {}
    for name, cases, expected_bytes in [("profiles", 256, 682732), ("headers", 1024, 12288), ("sessions", 64, 4366848)]:
        data = (args.output_dir / f"{name}.bin").read_bytes()
        if len(data) != expected_bytes:
            raise ValueError(f"Incomplete native oracle output: {name}: {len(data)}")
        outputs[name] = dict(cases=cases, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    report = dict(schemaVersion=1, referenceCommit=PIN, sourceGitBlobs=SOURCES,
        profileSeed=0xa2ca5e01, headerSeed=0xa2ca5e02, profileBytes=0x374, payloadBytes=0x378,
        checksumOffset=0x374, timeRecords=315, nameWords=6, campaignSectors=10, sectorBytes=4096,
        sessionScenarios=8, sessionSteps=8, sessionHeaderWords=10, functions=names, outputs=outputs,
        boundary="Exact original C; bool8=u8 and native 32-bit checksum words; Random/flash are deterministic in-memory stubs, no hardware or real campaign-save fixture")
    args.summary.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
