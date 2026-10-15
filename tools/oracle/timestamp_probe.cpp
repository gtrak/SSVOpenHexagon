// Oracle probe: `hg::Utils::formatTimepoint` via `put_time` (gmtime/UTC)
// with the two format strings the game uses (Replay.cpp, MenuGame.cpp).
// Pin these values in `oh-math` (hg::timestamp::format_timepoint).
#include <cstdio>
#include <chrono>
#include <cstdint>
#include <iomanip>
#include <sstream>
#include <string>

int main()
{
    uint64_t tss[] = {
        0, 1, 42, 86400, 86401, 1000000, 1700000000, 1700000001,
        1735689600, 1830000000, 2147483647, 2147483648, 4102444800,
        4294967295, 31556735999, 10000000000, 1767225600, 1699999999};
    for (uint64_t ts : tss)
    {
        auto tp =
            std::chrono::system_clock::time_point{} + std::chrono::seconds(ts);
        std::time_t tt = std::chrono::system_clock::to_time_t(tp);
        std::tm tm = *std::gmtime(&tt); // GMT (UTC), same as Timestamp.cpp
        for (const char* fmt :
            {
                "%Y%m%d_%H%M%S", // Replay.cpp create_filename
                "%Y-%m-%d %H:%M:%S", // MenuGame score display
            })
        {
            std::stringstream ss;
            ss << std::put_time(&tm, fmt);
            printf("ts=%llu\t%s\t%s\n", (unsigned long long)ts, fmt,
                ss.str().c_str());
        }
    }
    return 0;
}
