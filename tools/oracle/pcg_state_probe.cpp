#include "SSVUtils/Internal/PCG/pcg_random.hpp"
#include <cstdio>
int main(){
    // Task 030b fingerprint ground truth: the canonical pcg32_fast MCG state
    // position. `pcg32_fast` (mcg_xsh_rs_64_32, no_stream, increment()==0,
    // multiplier M=6364136223846793005) keeps states in the coset
    // `== 3 (mod 4)`; the wrapped state is 3. The public `operator-`
    // (pcg_random.hpp) is `lhs - rhs == rhs.distance(lhs.state_)`, so
    //   `f - pcg32_fast{3}`
    // is the number of engine bumps d in [0, 2^62) with 3 * M^d ==
    // f.state (mod 2^64) — a pure-integer, transcendental-free bijection of
    // the state coset onto [0, 2^62) (the bit-by-bit `engine::distance`,
    // is_mcg path; `distance` itself is protected, `operator-` is public).
    //
    // The fingerprint's field 15 ("game-RNG state") is this label (see
    // .agents/plans/001-rust-port/parity/cpp-fingerprint-definition.md);
    // the port computes the identical loop (oh_rng::pcg).
    //
    // Wrapper semantics (random_number_generator ctor): engine{seed} then
    // advance(1).
    unsigned long long seeds[] = {0ULL, 1ULL, 12345ULL, 987654321ULL, 4294967295ULL};
    unsigned long long extras[] = {0ULL, 1ULL, 2ULL, 3ULL, 7ULL, 63ULL, 100ULL, 1000ULL};
    for(unsigned long long seed : seeds){
        for(unsigned long long extra : extras){
            pcg32_fast f{seed};
            f.advance(1 + extra);
            printf("statepos %llu %llu %llu\n",
                seed, extra, (unsigned long long)(f - pcg32_fast{3}));
        }
    }
    // Linearity check: label(state_{k}) - label(state_{k'}) == k - k'
    // (mod 2^62) (the label is the bump count from the wrapped state).
    pcg32_fast a{12345}; a.advance(1);
    pcg32_fast b{12345}; b.advance(1 + 1000);
    unsigned long long lab_a = (unsigned long long)(a - pcg32_fast{3});
    unsigned long long lab_b = (unsigned long long)(b - pcg32_fast{3});
    printf("linearity %llu\n", (lab_b - lab_a) % (1ULL << 62));
    return 0;
}
