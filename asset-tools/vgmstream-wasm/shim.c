// Minimal C API over libvgmstream for the browser build (wasm32-wasi via zig cc).
// One open stream per handle; the file lives in wasm memory, handed over by JS.
// vgmstream (ISC licence) is credited in THIRD_PARTY_NOTICES.md.
#include <stdlib.h>
#include <string.h>
#include "libvgmstream.h"
#include "libvgmstream_streamfile.h"

#define EXPORT(name) __attribute__((export_name(#name)))

typedef struct {
    const uint8_t* data; // owned by the handle (malloc'd by JS through va_alloc)
    int64_t size;
    char name[256];
    int refs;            // vgmstream reopens the same "file"; all opens share the buffer
} mem_file_t;

static int mf_read(void* u, uint8_t* dst, int64_t off, int len) {
    mem_file_t* f = u;
    if (off < 0 || len <= 0 || off >= f->size) return 0;
    if (len > f->size - off) len = (int)(f->size - off);
    memcpy(dst, f->data + off, len);
    return len;
}
static int64_t mf_size(void* u) { return ((mem_file_t*)u)->size; }
static const char* mf_name(void* u) { return ((mem_file_t*)u)->name; }
static void mf_close(libstreamfile_t* sf);

static libstreamfile_t* mf_wrap(mem_file_t* f) {
    libstreamfile_t* sf = calloc(1, sizeof(*sf));
    if (!sf) return NULL;
    f->refs++;
    sf->user_data = f;
    sf->read = mf_read;
    sf->get_size = mf_size;
    sf->get_name = mf_name;
    sf->close = mf_close;
    extern libstreamfile_t* mf_open(void*, const char*);
    sf->open = mf_open;
    return sf;
}
// Only the file itself can be reopened: companion files are not available here.
libstreamfile_t* mf_open(void* u, const char* filename) {
    mem_file_t* f = u;
    if (!filename || strcmp(filename, f->name) != 0) return NULL;
    return mf_wrap(f);
}
static void mf_close(libstreamfile_t* sf) {
    mem_file_t* f = sf->user_data;
    free(sf);
    if (--f->refs == 0) { free((void*)f->data); free(f); }
}

typedef struct {
    libvgmstream_t* lib;
} va_t;

EXPORT(va_alloc) void* va_alloc(int size) { return malloc(size > 0 ? size : 1); }
EXPORT(va_free) void va_free(void* p) { free(p); }

// Takes ownership of `data` (from va_alloc). `name` gives the extension vgmstream
// uses to pick a format, e.g. "bgm.adx", "0.csb", "voice.afs".
// subsong: 0 = default/first. loop: play forever when the file has loop points.
EXPORT(va_open) va_t* va_open(uint8_t* data, int size, const char* name, int subsong, int loop) {
    mem_file_t* f = calloc(1, sizeof(*f));
    if (!f) { free(data); return NULL; }
    f->data = data;
    f->size = size;
    strncpy(f->name, name ? name : "file", sizeof(f->name) - 1);
    libstreamfile_t* sf = mf_wrap(f); // refs = 1, freed with the last close
    if (!sf) { free(data); free(f); return NULL; }

    libvgmstream_config_t cfg = {0};
    cfg.allow_play_forever = true;
    cfg.play_forever = loop != 0;
    cfg.ignore_loop = loop == 0;
    cfg.force_sfmt = LIBVGMSTREAM_SFMT_FLOAT;
    cfg.auto_downmix_channels = 2;

    libvgmstream_t* lib = libvgmstream_create(sf, subsong, &cfg);
    sf->close(sf); // vgmstream keeps its own reopened handles
    if (!lib) return NULL;
    va_t* va = calloc(1, sizeof(*va));
    if (!va) { libvgmstream_free(lib); return NULL; }
    va->lib = lib;
    return va;
}

EXPORT(va_close) void va_close(va_t* va) {
    if (!va) return;
    libvgmstream_free(va->lib);
    free(va);
}

EXPORT(va_channels) int va_channels(va_t* va) { return va->lib->format->channels; }
EXPORT(va_rate) int va_rate(va_t* va) { return va->lib->format->sample_rate; }
EXPORT(va_subsongs) int va_subsongs(va_t* va) { return va->lib->format->subsong_count; }
EXPORT(va_samples) double va_samples(va_t* va) { return (double)va->lib->format->stream_samples; }
// ignore_loop clears loop_flag but keeps the points, so report loops from the points.
EXPORT(va_loop_flag) int va_loop_flag(va_t* va) { return va->lib->format->loop_end > va->lib->format->loop_start; }
EXPORT(va_loop_start) double va_loop_start(va_t* va) { return (double)va->lib->format->loop_start; }
EXPORT(va_loop_end) double va_loop_end(va_t* va) { return (double)va->lib->format->loop_end; }
EXPORT(va_codec) const char* va_codec(va_t* va) { return va->lib->format->codec_name; }
EXPORT(va_meta) const char* va_meta(va_t* va) { return va->lib->format->meta_name; }
EXPORT(va_stream_name) const char* va_stream_name(va_t* va) { return va->lib->format->stream_name; }

// Decode up to `frames` interleaved float frames into `out`. Returns frames written,
// 0 at the end of the stream, -1 on error.
EXPORT(va_fill) int va_fill(va_t* va, float* out, int frames) {
    if (frames <= 0) return 0;
    int err = libvgmstream_fill(va->lib, out, frames);
    if (err < 0) return -1;
    return va->lib->decoder->buf_samples;
}

EXPORT(va_seek) void va_seek(va_t* va, double sample) { libvgmstream_seek(va->lib, (int64_t)sample); }
