/* What meridian-seedatlas needs of the engine that the engine only offers as
 * inline functions. */
#include "finders.h"

int meridian_is_slime_chunk(uint64_t seed, int chunk_x, int chunk_z)
{
    return isSlimeChunk(seed, chunk_x, chunk_z);
}
