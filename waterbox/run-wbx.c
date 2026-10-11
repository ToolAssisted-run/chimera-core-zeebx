#define _GNU_SOURCE
/* run-wbx - runs core.wbx in Chimera's sandbox (miniBox) the way the engine
 * does, taking run-native's options and printing run-native's lines, so that
 * the two outputs can be compared directly.
 *
 *   run-wbx <core.wbx> <game> [run-native's options] [--rerecord]
 *           [--save-state FILE --at FRAME] [--state FILE] [--savedata ZIP]
 *
 * The game is mounted under its own name without being read, with "rom.name"
 * saying which and "settings" beside it, as the engine mounts them.
 *
 *   --rerecord            save the whole machine and load it again before
 *                         every frame; the lines must not change
 *   --save-state F --at N save the machine to F after frame N
 *   --shot N=FILE         write the picture as it is after frame N (repeatable)
 *   --state F             load the machine from F before the first frame
 *                         (made by another process with the same options),
 *                         and count frames from where it was saved
 */
#include "minibox.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define WIDTH 640
#define HEIGHT 480

static mb_host *g_host;

/* FNV-1a, as run-native has it */
#define FNV_START 0xcbf29ce484222325ull
static uint64_t fnv(uint64_t h, const uint8_t *p, size_t n)
{
	for (size_t i = 0; i < n; i++) {
		h ^= p[i];
		h *= 0x100000001b3ull;
	}
	return h;
}

typedef struct { FILE *f; } freader;
static intptr_t file_read(uintptr_t ud, uint8_t *data, uintptr_t size)
{
	return (intptr_t)fread(data, 1, size, ((freader *)ud)->f);
}
typedef struct { uint8_t *b; size_t len, cap, pos; } membuf;
static int32_t mem_write(uintptr_t ud, const uint8_t *d, uintptr_t n)
{
	membuf *m = (membuf *)ud;
	if (m->len + n > m->cap) {
		size_t cap = m->cap ? m->cap * 2 : ((size_t)16 << 20);
		while (cap < m->len + n)
			cap *= 2;
		uint8_t *b = (uint8_t *)realloc(m->b, cap);
		if (!b)
			return -1;
		m->b = b;
		m->cap = cap;
	}
	memcpy(m->b + m->len, d, n);
	m->len += n;
	return 0;
}
static intptr_t mem_read(uintptr_t ud, uint8_t *d, uintptr_t n)
{
	membuf *m = (membuf *)ud;
	size_t left = m->len - m->pos;
	if (n > left)
		n = left;
	memcpy(d, m->b + m->pos, n);
	m->pos += n;
	return (intptr_t)n;
}

static void must(const mb_return *r, const char *what)
{
	if (r->error_message[0]) {
		fprintf(stderr, "run-wbx: %s: %s\n", what, (const char *)r->error_message);
		exit(1);
	}
}
static uintptr_t proc(const char *name)
{
	mb_return r;
	wbx_get_proc_addr(g_host, name, &r);
	if (r.error_message[0] || !r.data) {
		fprintf(stderr, "run-wbx: the core has no %s %s\n", name, (const char *)r.error_message);
		exit(2);
	}
	return r.data;
}
static void mount_text(const char *name, const char *text)
{
	membuf m = { (uint8_t *)text, strlen(text), strlen(text), 0 };
	mb_return r;
	wbx_mount_file(g_host, name, mem_read, (uintptr_t)&m, false, &r);
	must(&r, name);
}
static void save(membuf *out)
{
	mb_return r;
	out->len = 0;
	wbx_save_state(g_host, mem_write, (uintptr_t)out, &r);
	must(&r, "save");
}
static void load(const uint8_t *b, size_t len)
{
	membuf m = { (uint8_t *)b, len, len, 0 };
	mb_return r;
	wbx_load_state(g_host, mem_read, (uintptr_t)&m, &r);
	must(&r, "load");
}

struct press { long from, count; uint64_t mask; };
struct shot { long frame; const char *path; };

static int write_ppm(const char *path, const uint32_t *video)
{
	FILE *o = fopen(path, "wb");
	if (!o)
		return 0;
	fprintf(o, "P6\n%d %d\n255\n", WIDTH, HEIGHT);
	for (int i = 0; i < WIDTH * HEIGHT; i++) {
		const uint8_t rgb[3] = { (uint8_t)(video[i] >> 16), (uint8_t)(video[i] >> 8), (uint8_t)video[i] };
		fwrite(rgb, 1, 3, o);
	}
	fclose(o);
	return 1;
}

int main(int argc, char **argv)
{
	if (argc < 3) {
		fprintf(stderr, "usage: run-wbx <core.wbx> <game> [--frames N] [--every N] [--press F:N:MASK]... [--pads N] "
		                "[--rtc SECONDS] [--ppm FILE] [--rerecord] [--save-state FILE --at FRAME] [--state FILE]\n");
		return 2;
	}
	long frames = 600, every = 60, at = 0;
	const char *ppm = NULL, *save_state = NULL, *state = NULL, *savedata = NULL;
	int rerecord = 0, npress = 0;
	struct press presses[64];
	struct shot shots[16];
	int nshot = 0;
	char settings[512] = "{";
	size_t sl = 1;
	for (int i = 3; i < argc; i++) {
		const char *a = argv[i];
		if (!strcmp(a, "--rerecord")) {
			rerecord = 1;
			continue;
		}
		const char *v = i + 1 < argc ? argv[++i] : NULL;
		if (!v) {
			fprintf(stderr, "%s needs a value\n", a);
			return 2;
		}
		if (!strcmp(a, "--frames")) frames = atol(v);
		else if (!strcmp(a, "--every")) every = atol(v);
		else if (!strcmp(a, "--ppm")) ppm = v;
		else if (!strcmp(a, "--save-state")) save_state = v;
		else if (!strcmp(a, "--at")) at = atol(v);
		else if (!strcmp(a, "--state")) state = v;
		else if (!strcmp(a, "--savedata")) savedata = v;
		else if (!strcmp(a, "--shot")) {
			char *eq = strchr(v, '=');
			if (!eq || nshot == 16) {
				fprintf(stderr, "--shot FRAME=FILE\n");
				return 2;
			}
			shots[nshot].frame = atol(v);
			shots[nshot++].path = eq + 1;
		}
		else if (!strcmp(a, "--pads")) sl += (size_t)snprintf(settings + sl, sizeof settings - sl, "\"pads\":\"%s\",", v);
		else if (!strcmp(a, "--rtc")) sl += (size_t)snprintf(settings + sl, sizeof settings - sl, "\"rtc_start\":\"%s\",", v);
		else if (!strcmp(a, "--press")) {
			unsigned long long mask;
			if (npress == 64 || sscanf(v, "%ld:%ld:%llx", &presses[npress].from, &presses[npress].count, &mask) != 3) {
				fprintf(stderr, "--press F:N:MASK\n");
				return 2;
			}
			presses[npress++].mask = mask;
		} else {
			fprintf(stderr, "unknown option %s\n", a);
			return 2;
		}
	}
	if (settings[sl - 1] == ',')
		sl--;
	snprintf(settings + sl, sizeof settings - sl, "}");

	FILE *f = fopen(argv[1], "rb");
	if (!f) {
		fprintf(stderr, "run-wbx: cannot open %s\n", argv[1]);
		return 2;
	}
	/* The emulator's own memory and the unpacked game are on the heap; the
	 * recompiler's code is mapped. Nothing is committed until it is touched. */
	mb_memory_layout_template layout = {
		.sbrk_size = 2048ull << 20, .sealed_size = 16ull << 20, .invis_size = 16ull << 20,
		.plain_size = 64ull << 20, .mmap_size = 4096ull << 20,
	};
	freader fr = { f };
	mb_return r;
	wbx_create_host(&layout, "core.wbx", file_read, (uintptr_t)&fr, &r);
	fclose(f);
	must(&r, "create_host");
	g_host = (mb_host *)r.data;

	const char *base = strrchr(argv[2], '/');
	base = base ? base + 1 : argv[2];
	wbx_mount_file_path(g_host, base, argv[2], &r);
	must(&r, argv[2]);
	mount_text("rom.name", base);
	if (savedata) {
		/* as a project mounts it: the zip under its own name, named in the
		 * "savedata" slot of the list of the project's files */
		const char *sbase = strrchr(savedata, '/');
		sbase = sbase ? sbase + 1 : savedata;
		wbx_mount_file_path(g_host, sbase, savedata, &r);
		must(&r, savedata);
		static char slots[1024];
		snprintf(slots, sizeof slots, "{\"game\":[\"%s\"],\"savedata\":[\"%s\"]}", base, sbase);
		mount_text("slots", slots);
	}
	mount_text("settings", settings);
	wbx_activate_host(g_host, &r);
	must(&r, "activate");

	/* on Linux the sandboxed core's calling convention is the host's: its
	 * functions are called as plain functions */
	int (*Init)(void) = (int (*)(void))proc("Init");
	const char *(*GetLoadError)(void) = (const char *(*)(void))proc("GetLoadError");
	void (*FrameAdvance)(uint64_t) = (void (*)(uint64_t))proc("FrameAdvance");
	int (*IsRunning)(void) = (int (*)(void))proc("IsRunning");
	uint64_t (*GetMachineTimeNs)(void) = (uint64_t (*)(void))proc("GetMachineTimeNs");
	uint64_t (*GetExecutedTicks)(void) = (uint64_t (*)(void))proc("GetExecutedTicks");
	const uint32_t *(*GetVideoBgra)(void) = (const uint32_t *(*)(void))proc("GetVideoBgra");
	const int16_t *(*GetAudio)(void) = (const int16_t *(*)(void))proc("GetAudio");
	int (*GetAudioSampleCount)(void) = (int (*)(void))proc("GetAudioSampleCount");

	if (!Init()) {
		fprintf(stderr, "run-wbx: Init failed: %s\n", GetLoadError());
		return 1;
	}
	/* the machine as Init left it is what every state is taken against */
	wbx_deactivate_host(g_host, &r);
	wbx_seal(g_host, &r);
	must(&r, "seal");
	wbx_activate_host(g_host, &r);

	long first = 1;
	uint64_t sound = FNV_START, pairs = 0;
	if (state) {
		FILE *s = fopen(state, "rb");
		if (!s) {
			fprintf(stderr, "run-wbx: cannot open %s\n", state);
			return 2;
		}
		/* the file: the frame it was saved after, the sound digest and count
		 * so far, then the machine */
		uint64_t head[3];
		if (fread(head, sizeof head, 1, s) != 1)
			return 2;
		fseek(s, 0, SEEK_END);
		long size = ftell(s) - (long)sizeof head;
		fseek(s, sizeof head, SEEK_SET);
		uint8_t *b = (uint8_t *)malloc((size_t)size);
		if (fread(b, 1, (size_t)size, s) != (size_t)size)
			return 2;
		fclose(s);
		load(b, (size_t)size);
		free(b);
		first = (long)head[0] + 1;
		sound = head[1];
		pairs = head[2];
	}

	membuf scratch = { 0 };
	for (long frame = first; frame <= frames; frame++) {
		if (rerecord) {
			/* RUN_WBX_BREAK_STALE_STATE=1, for seeing the test fail: half way
			 * through, the state loaded is the one saved a frame earlier */
			static membuf earlier;
			const char *brk = getenv("RUN_WBX_BREAK_STALE_STATE");
			if (brk && *brk && frame == (first + frames) / 2 && earlier.len) {
				load(earlier.b, earlier.len);
			} else {
				save(&scratch);
				load(scratch.b, scratch.len);
			}
			if (brk && *brk) {
				earlier.len = 0;
				mem_write((uintptr_t)&earlier, scratch.b, scratch.len);
			}
		}
		uint64_t buttons = 0;
		for (int p = 0; p < npress; p++)
			if (frame >= presses[p].from && frame < presses[p].from + presses[p].count)
				buttons |= presses[p].mask;
		FrameAdvance(buttons);
		const int count = GetAudioSampleCount();
		sound = fnv(sound, (const uint8_t *)GetAudio(), (size_t)count * 4);
		pairs += (uint64_t)count;
		const int running = IsRunning();
		if (frame % every == 0 || frame == frames || !running) {
			printf("frame=%ld clockNs=%llu instructions=%llu audioPairs=%llu audioHash=%016llx video=%dx%d videoHash=%016llx\n",
			       frame, (unsigned long long)GetMachineTimeNs(), (unsigned long long)GetExecutedTicks(),
			       (unsigned long long)pairs, (unsigned long long)sound, WIDTH, HEIGHT,
			       (unsigned long long)fnv(FNV_START, (const uint8_t *)GetVideoBgra(), WIDTH * HEIGHT * 4));
		}
		for (int k = 0; k < nshot; k++)
			if (shots[k].frame == frame && !write_ppm(shots[k].path, GetVideoBgra()))
				return 2;
		if (!running) {
			printf("stopped at frame %ld\n", frame);
			break;
		}
		if (save_state && frame == at) {
			save(&scratch);
			FILE *s = fopen(save_state, "wb");
			uint64_t head[3] = { (uint64_t)frame, sound, pairs };
			if (!s || fwrite(head, sizeof head, 1, s) != 1 || fwrite(scratch.b, 1, scratch.len, s) != scratch.len) {
				fprintf(stderr, "run-wbx: cannot write %s\n", save_state);
				return 2;
			}
			fclose(s);
			fprintf(stderr, "run-wbx: saved after frame %ld (%zu bytes)\n", frame, scratch.len);
		}
	}
	/* the game's memory, as RAM tools see it: run-native's line */
	{
		const int64_t *(*GetBusRanges)(int) = (const int64_t *(*)(int))proc("GetBusRanges");
		const uint8_t *(*ReadBus)(int, int64_t, int) = (const uint8_t *(*)(int, int64_t, int))proc("ReadBus");
		int (*PeekBus)(int, int) = (int (*)(int, int))proc("PeekBus");
		void (*PokeBus)(int, int, int) = (void (*)(int, int, int))proc("PokeBus");
		const int64_t *ranges = GetBusRanges(0);
		/* the list is the core's until the next call: copy it */
		int64_t list[128];
		int n = 0;
		while (n < 63 && ranges[2 * n + 1] > 0) {
			list[2 * n] = ranges[2 * n];
			list[2 * n + 1] = ranges[2 * n + 1];
			n++;
		}
		uint64_t bytes = 0, digest = FNV_START;
		for (int i = 0; i < n; i++) {
			for (int64_t at = 0; at < list[2 * i + 1];) {
				int64_t len = list[2 * i + 1] - at;
				if (len > 65536)
					len = 65536;
				digest = fnv(digest, ReadBus(0, list[2 * i] + at, (int)len), (size_t)len);
				at += len;
			}
			bytes += (uint64_t)list[2 * i + 1];
		}
		const char *poke = "no memory";
		if (n > 0) {
			const int before = PeekBus(0, (int)list[0]);
			PokeBus(0, (int)list[0], before ^ 0xff);
			poke = PeekBus(0, (int)list[0]) == (before ^ 0xff) ? "a written byte reads back" : "a written byte does NOT read back";
			PokeBus(0, (int)list[0], before);
		}
		printf("bus ranges=%d bytes=%llu hash=%016llx (%s)\n", n, (unsigned long long)bytes, (unsigned long long)digest, poke);
	}
	/* what the console keeps: run-native's lines */
	{
		int (*Count)(void) = (int (*)(void))proc("GetSaveDataFileCount");
		const char *(*Name)(int) = (const char *(*)(int))proc("GetSaveDataFileName");
		uint64_t (*Size)(int) = (uint64_t (*)(int))proc("GetSaveDataFileSize");
		const uint8_t *(*Buffer)(int) = (const uint8_t *(*)(int))proc("GetSaveDataFileBuffer");
		const int n = Count();
		for (int i = 0; i < n; i++) {
			const uint64_t size = Size(i);
			printf("save %s %llu %016llx\n", Name(i), (unsigned long long)size,
			       (unsigned long long)fnv(FNV_START, Buffer(i), (size_t)size));
		}
	}
	if (ppm && !write_ppm(ppm, GetVideoBgra()))
		return 2;
	fflush(stdout);
	return 0;
}
