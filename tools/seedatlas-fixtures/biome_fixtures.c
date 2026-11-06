/* Writes biome fixtures from the Fabric version's native interface
 * (seedatlas_xaero.c over the Seed Atlas engine) for Meridian's parity
 * tests. Development tool only: not part of the mod.
 *
 * Lines:
 *   P seed large dim x y z id
 *   A seed large dim ox oz y step w h id...
 *   C seed large dim cx cz y step id...   (256 ids)
 */
#include "seedatlas_xaero.h"
#include <stdio.h>
#include <stdlib.h>

static const int64_t SEEDS[] = { 0, 1, -4172144997902289642LL, 123456789, 8675309, -1 };
static const int DIMS[] = { SAX_DIM_OVERWORLD, SAX_DIM_NETHER, SAX_DIM_END };

static uint64_t rng = 0x9E3779B97F4A7C15ULL;
static int32_t rnd(int32_t lo, int32_t hi)
{
    rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
    return lo + (int32_t)(rng % (uint64_t)(hi - lo + 1));
}

static void area(sax_context *c, int64_t seed, int large, int dim, int ox, int oz, int y, int step, int w, int h)
{
    int32_t *ids = malloc(sizeof(int32_t) * w * h);
    if (sax_biome_area_sampled(c, dim, ox, oz, y, step, w, h, ids, NULL, w * h) != SAX_OK) {
        fprintf(stderr, "area failed\n");
        exit(1);
    }
    printf("A %lld %d %d %d %d %d %d %d %d", (long long) seed, large, dim, ox, oz, y, step, w, h);
    for (int i = 0; i < w * h; i++)
        printf(" %d", ids[i]);
    printf("\n");
    free(ids);
}

int main(void)
{
    for (size_t s = 0; s < sizeof(SEEDS) / sizeof(*SEEDS); s++) {
        for (int large = 0; large <= 1; large++) {
            int64_t seed = SEEDS[s];
            if (large && s > 1)
                continue;
            sax_context *c = sax_create(seed, large ? SAX_WORLD_LARGE_BIOMES : 0);
            for (int d = 0; d < 3; d++) {
                int dim = DIMS[d];
                for (int i = 0; i < 60; i++) {
                    int32_t x = rnd(-30000, 30000), y = rnd(-64, 320), z = rnd(-30000, 30000), id = -9;
                    if (i % 3 == 0) { x = rnd(-3000000, 3000000); z = rnd(-3000000, 3000000); }
                    sax_biome_at(c, dim, x, y, z, &id);
                    printf("P %lld %d %d %d %d %d %d\n", (long long) seed, large, dim, x, y, z, id);
                }
                area(c, seed, large, dim, rnd(-2000, 2000), rnd(-2000, 2000), 64, 1, 40, 24);
                area(c, seed, large, dim, rnd(-2000, 2000), rnd(-2000, 2000), -30, 1, 20, 20);
                area(c, seed, large, dim, rnd(-2000, 2000), rnd(-2000, 2000), 64, 2, 24, 24);
                area(c, seed, large, dim, rnd(-500, 500) * 4, rnd(-500, 500) * 4, 64, 4, 32, 32);
                area(c, seed, large, dim, rnd(-500, 500) * 8, rnd(-500, 500) * 8, 64, 8, 32, 32);
                area(c, seed, large, dim, rnd(-500, 500) * 16, rnd(-500, 500) * 16, 64, 16, 32, 32);
                area(c, seed, large, dim, rnd(-500, 500) * 64, rnd(-500, 500) * 64, 70, 64, 32, 32);
                area(c, seed, large, dim, rnd(-100, 100) * 256, rnd(-100, 100) * 256, 64, 256, 24, 24);
                area(c, seed, large, dim, rnd(-50, 50) * 1024, rnd(-50, 50) * 1024, 64, 1024, 12, 12);
                for (int step = 1; step <= 16; step *= 2) {
                    int32_t ids[256];
                    int cx = rnd(-2000, 2000), cz = rnd(-2000, 2000);
                    if (sax_biome_chunk_sampled(c, dim, cx, cz, 64, step, ids, NULL) != SAX_OK)
                        return 1;
                    printf("C %lld %d %d %d %d %d %d", (long long) seed, large, dim, cx, cz, 64, step);
                    for (int i = 0; i < 256; i++)
                        printf(" %d", ids[i]);
                    printf("\n");
                }
            }
            sax_destroy(c);
        }
    }
    return 0;
}
