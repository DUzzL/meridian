/* Writes structure fixtures from the Fabric version's native interface for
 * Meridian's parity tests. Development tool only: not part of the mod.
 *
 * Lines:
 *   S seed kind check x0 z0 x1 z1 count (kind x y z flags detail)...
 *   H seed check x0 z0 x1 z1 count (...)          strongholds
 *   W seed detailed x z                           spawn
 *   M seed x0 z0 x1 z1 count (...)                slime chunks
 */
#include "seedatlas_xaero.h"
#include <stdio.h>
#include <stdlib.h>

static const int64_t SEEDS[] = { 0, 1, -4172144997902289642LL, 123456789, 8675309 };
static int32_t buf[6 * 200000];

static void rows(int n)
{
    printf(" %d", n);
    for (int i = 0; i < n * 6; i++)
        printf(" %d", buf[i]);
    printf("\n");
}

static void scan(sax_context *c, int64_t seed, int kind, int check, int r)
{
    int n = sax_scan_structures(c, kind, -r, -r, r, r, check, buf, 200000);
    if (n < 0 || n > 200000) { fprintf(stderr, "scan %d failed %d\n", kind, n); exit(1); }
    printf("S %lld %d %d %d %d %d %d", (long long) seed, kind, check, -r, -r, r, r);
    rows(n);
}

int main(void)
{
    for (size_t s = 0; s < sizeof(SEEDS) / sizeof(*SEEDS); s++) {
        int64_t seed = SEEDS[s];
        sax_context *c = sax_create(seed, 0);
        for (int kind = 1; kind <= 25; kind++) {
            int r = 12000;
            if (kind == 9) r = 60000;                         /* mansions are sparse */
            if (kind == 14 || kind == 15 || kind == 16 || kind == 17) r = 1500; /* per chunk */
            if (kind == 21 || kind == 22) r = 1500;
            if (kind == 20) r = 8000;
            for (int check = 0; check <= 1; check++)
                scan(c, seed, kind, check, r);
        }
        scan(c, seed, 1006, 1, 8000);
        scan(c, seed, 1007, 0, 1200);
        scan(c, seed, 1004, 0, 800);
        for (int check = 0; check <= 1; check++) {
            int n = sax_scan_strongholds(c, -200000, -200000, 200000, 200000, check, buf, 200000);
            printf("H %lld %d %d %d %d %d", (long long) seed, check, -200000, -200000, 200000, 200000);
            rows(n);
        }
        for (int detailed = 0; detailed <= 1; detailed++) {
            int32_t xyz[3];
            sax_spawn(c, detailed, xyz);
            printf("W %lld %d %d %d\n", (long long) seed, detailed, xyz[0], xyz[2]);
        }
        int n = sax_scan_slime_chunks(c, -1000, -700, 900, 1100, buf, 200000);
        printf("M %lld %d %d %d %d", (long long) seed, -1000, -700, 900, 1100);
        rows(n);
        sax_destroy(c);
    }
    return 0;
}
