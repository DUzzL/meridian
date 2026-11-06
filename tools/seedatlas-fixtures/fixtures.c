/* Writes parity fixtures from the Fabric version's native interface (the
 * seedatlas_xaero glue on DUzzL/seed-atlas-xaero main, ABI 6, over the Seed
 * Atlas engine) for every Minecraft version it accepts. Development tool
 * only: not part of the mod.
 *
 * Usage: fixtures <version> > <version>.txt   (one file per version)
 *
 * Lines (dimension -1 Nether, 0 Overworld, 1 End):
 *   V version mc_supported has_terrain
 *   K id name argb                               context biome names/colours
 *   P seed large dim x y z id                    sax_biome_at
 *   A seed large dim ox oz y step w h id...      sax_biome_area_sampled
 *   C seed large dim cx cz y step id...          sax_biome_chunk_sampled
 *   T seed large dim ox oz step w h height...    sax_height_area_sampled
 *   N seed ox oz y step w h floor...             sax_nether_floor_area_sampled
 *   R seed ox oz y step w h mask... : floor...   ..._carved_sampled (2 words a sample)
 *   S seed kind check x0 z0 x1 z1 rc row...      sax_scan_structures (rc, then 6 per row)
 *   H seed check x0 z0 x1 z1 rc row...           sax_scan_strongholds
 *   W seed detailed x z                          sax_spawn
 *   M seed x0 z0 x1 z1 rc row...                 sax_scan_slime_chunks
 */
#include "seedatlas_xaero.h"
#include <stdio.h>
#include <stdlib.h>

static const int64_t SEEDS[] = { 0, -4172144997902289642LL, 8675309 };
static const int DIMS[] = { SAX_DIM_OVERWORLD, SAX_DIM_NETHER, SAX_DIM_END };
static int32_t buf[6 * 100000];
static int32_t ids[1 << 16];
static float heights[1 << 16];
static uint64_t mask[1 << 13];
static uint64_t rng = 0x9E3779B97F4A7C15ULL;

static int32_t rnd(int32_t lo, int32_t hi)
{
    rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
    return lo + (int32_t)(rng % (uint64_t)(hi - lo + 1));
}

static void rows(int n)
{
    printf(" %d", n);
    for (int i = 0; n > 0 && i < n * 6; i++)
        printf(" %d", buf[i]);
    printf("\n");
}

static void floats(int n)
{
    for (int i = 0; i < n; i++)
        printf(" %.9g", heights[i]);
}

int main(int argc, char **argv)
{
    const char *v = argc > 1 ? argv[1] : "26.3";
    int supported = sax_mc_version_supported(v);
    printf("V %s %d", v, supported);
    if (!supported) { printf(" 0\n"); return 0; }
    for (size_t s = 0; s < sizeof(SEEDS) / sizeof(*SEEDS); s++) {
        for (int large = 0; large <= 1; large++) {
            int64_t seed = SEEDS[s];
            if (large && s > 0)
                continue;
            sax_context *c = sax_create_versioned(seed, large ? SAX_WORLD_LARGE_BIOMES : 0, v);
            if (!c) { fprintf(stderr, "no context for %s\n", v); return 1; }
            if (s == 0 && !large) {
                printf(" %d\n", sax_context_has_terrain(c));
                for (int id = 0; id < 256; id++) {
                    char name[128];
                    if (sax_context_biome_name(c, id, name, sizeof name) > 0)
                        printf("K %d %s %u\n", id, name, sax_context_biome_color(c, id));
                }
            }
            for (int d = 0; d < 3; d++) {
                int dim = DIMS[d];
                for (int i = 0; i < 24; i++) {
                    int32_t x = rnd(-30000, 30000), y = rnd(-64, 320), z = rnd(-30000, 30000), id = -9;
                    if (i % 4 == 0) { x = rnd(-3000000, 3000000); z = rnd(-3000000, 3000000); }
                    sax_biome_at(c, dim, x, y, z, &id);
                    printf("P %lld %d %d %d %d %d %d\n", (long long) seed, large, dim, x, y, z, id);
                }
                static const int steps[] = { 1, 2, 4, 8, 16, 64, 256, 1024 };
                for (int k = 0; k < 8; k++) {
                    int step = steps[k], w = step <= 2 ? 20 : 16, h = w;
                    int ox = rnd(-400, 400) * (step < 4 ? 5 : step), oz = rnd(-400, 400) * (step < 4 ? 5 : step);
                    int y = k == 1 ? -30 : 64;
                    if (sax_biome_area_sampled(c, dim, ox, oz, y, step, w, h, ids, NULL, w * h) != SAX_OK) return 2;
                    printf("A %lld %d %d %d %d %d %d %d %d", (long long) seed, large, dim, ox, oz, y, step, w, h);
                    for (int i = 0; i < w * h; i++) printf(" %d", ids[i]);
                    printf("\n");
                }
                for (int step = 1; step <= 16; step *= 4) {
                    int cx = rnd(-2000, 2000), cz = rnd(-2000, 2000);
                    if (sax_biome_chunk_sampled(c, dim, cx, cz, 64, step, ids, NULL) != SAX_OK) return 3;
                    printf("C %lld %d %d %d %d %d %d", (long long) seed, large, dim, cx, cz, 64, step);
                    for (int i = 0; i < 256; i++) printf(" %d", ids[i]);
                    printf("\n");
                }
                if (sax_context_has_terrain(c) && dim != SAX_DIM_NETHER) {
                    static const int tsteps[] = { 1, 2, 4, 16, 64 };
                    for (int k = 0; k < 5; k++) {
                        int step = tsteps[k], w = 12, h = 12;
                        int ox = rnd(-3000, 3000), oz = rnd(-3000, 3000);
                        if (dim == SAX_DIM_END) { ox = rnd(-1600, 1600); oz = rnd(-1600, 1600); }
                        int rc = sax_height_area_sampled(c, dim, ox, oz, step, w, h, heights, w * h);
                        printf("T %lld %d %d %d %d %d %d %d %d", (long long) seed, large, dim, ox, oz, step, w, h, rc);
                        if (rc == SAX_OK) floats(w * h);
                        printf("\n");
                    }
                }
            }
            if (sax_context_has_terrain(c) && !large) {
                static const int nsteps[] = { 1, 2, 4, 8, 16 };
                for (int k = 0; k < 5; k++) {
                    int step = nsteps[k], w = 10, h = 10, y = k == 0 ? 127 : rnd(20, 120);
                    int ox = rnd(-2000, 2000) * (step >= 8 ? step : 1), oz = rnd(-2000, 2000) * (step >= 8 ? step : 1);
                    int rc = sax_nether_floor_area_sampled(c, ox, oz, y, step, w, h, heights, w * h);
                    printf("N %lld %d %d %d %d %d %d %d", (long long) seed, ox, oz, y, step, w, h, rc);
                    if (rc == SAX_OK) floats(w * h);
                    printf("\n");
                    for (int i = 0; i < w * h * 2; i++)
                        mask[i] = ((uint64_t) rnd(0, 0x7fffffff) << 33) ^ ((uint64_t) rnd(0, 0x7fffffff) << 2) ^ (uint64_t) rnd(0, 3);
                    rc = sax_nether_floor_area_carved_sampled(c, ox, oz, y, step, w, h, mask, w * h * 2, heights, w * h);
                    printf("R %lld %d %d %d %d %d %d %d", (long long) seed, ox, oz, y, step, w, h, rc);
                    for (int i = 0; i < w * h * 2; i++) printf(" %llu", (unsigned long long) mask[i]);
                    printf(" :");
                    if (rc == SAX_OK) floats(w * h);
                    printf("\n");
                }
            }
            if (!large) {
                static const int kinds[] = {1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,1006,1007,1004};
                for (size_t k = 0; k < sizeof(kinds) / sizeof(*kinds); k++) {
                    int kind = kinds[k], r = 3000;
                    if (kind == 9) r = 30000;
                    if (kind == 14 || kind == 15 || kind == 16 || kind == 17 || kind == 21 || kind == 22) r = 700;
                    if (kind >= 1004) r = 600;
                    for (int check = 0; check <= 1; check++) {
                        int n = sax_scan_structures(c, kind, -r, -r, r, r, check, buf, 100000);
                        printf("S %lld %d %d %d %d %d %d", (long long) seed, kind, check, -r, -r, r, r);
                        rows(n);
                    }
                }
                for (int check = 0; check <= 1; check++) {
                    int n = sax_scan_strongholds(c, -100000, -100000, 100000, 100000, check, buf, 100000);
                    printf("H %lld %d %d %d %d %d", (long long) seed, check, -100000, -100000, 100000, 100000);
                    rows(n);
                }
                for (int detailed = 0; detailed <= 1; detailed++) {
                    int32_t xyz[3];
                    sax_spawn(c, detailed, xyz);
                    printf("W %lld %d %d %d\n", (long long) seed, detailed, xyz[0], xyz[2]);
                }
                int n = sax_scan_slime_chunks(c, -500, -300, 400, 600, buf, 100000);
                printf("M %lld %d %d %d %d", (long long) seed, -500, -300, 400, 600);
                rows(n);
            }
            sax_destroy(c);
        }
    }
    return 0;
}
