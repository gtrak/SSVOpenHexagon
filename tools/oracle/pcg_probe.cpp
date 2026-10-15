#include "SSVUtils/Internal/PCG/pcg_random.hpp"
#include <cstdio>
#include <random>
int main(){
    // seed 12345 as used by random_number_generator
    pcg32_fast rng{12345u};
    std::uniform_int_distribution<int> di{0,5};
    std::uniform_real_distribution<float> dr{0.f,1.f};
    for(int i=0;i<8;i++) printf("raw=%u\n", (unsigned)rng());
    pcg32_fast r2{12345u};
    for(int i=0;i<8;i++) printf("int=%d\n", di(r2));
    pcg32_fast r3{12345u};
    for(int i=0;i<8;i++) printf("real=%.9g\n", (double)dr(r3));
    pcg32_fast r4{12345u};
    r4.advance(100);
    printf("adv100=%u\n", (unsigned)r4());
    pcg32_fast r5{0u};
    printf("seed0=%u\n", (unsigned)r5());
    pcg32_fast r6{4294967295u};
    printf("seedmax=%u\n", (unsigned)r6());
    // engine min/max
    printf("min=%u max=%u\n", (unsigned)pcg32_fast::min(), (unsigned)pcg32_fast::max());
    return 0;
}
