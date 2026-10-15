// Oracle probe: C++ default `std::ostream <<` formatting for float/double
// (glibc %g, precision 6) — the formatting that `hg::Utils::concat` uses.
// Pin these values in `oh-math` (hg::concat::format_ostream_f32/f64).
#include <cstdio>
#include <cstring>
#include <cmath>
#include <sstream>
#include <string>
#include <cstdint>

std::string fmt32(float v)
{
    std::ostringstream oss;
    oss << v;
    return oss.str();
}

std::string fmt64(double v)
{
    std::ostringstream oss;
    oss << v;
    return oss.str();
}

int main()
{
    // Fixed values: the concat oracle values + format-boundary cases.
    float fvals[] = {
        1.6f, 9.45f, 1.35f, 0.1f, 0.2f, 0.3f, 123456.5f, 999999.4f,
        1234567.0f, 0.0001f, 1e-7f, 3600.0f, 5999.999f, 0.0f, -0.0f,
        -1.6f, 100.0f, 100000.0f, 0.5f, 2.5f, 0.125f, 3.14159f,
        6.28318f, 1e10f, 1e-5f, 255.0f, 255.5f, 0.015625f, 7.2063e-05f,
        -123456.7f, -0.00001f, 12345678.0f};
    for (float v : fvals)
        printf("f32\t%.9g\t%s\n", (double)v, fmt32(v).c_str());

    double dvals[] = {1.6, 9.45, 1.35, 0.1, 3600.0, 5999.999, 123456.7,
        999999.4, 1234567.0, 0.0001, 1e-7, 0.0, -0.0, -1.6, 1e10, 1e-5,
        100000.0, 3.14159265, 1.0 / 3.0, 2.5, 0.5, 62.5, 1e-4, -0.00001,
        12345678.0, 999999.5, 2.675e-02, 1.0000001, 0.9999999, 1e-6};
    for (double v : dvals)
        printf("f64\t%.17g\t%s\n", v, fmt64(v).c_str());

    // Sweep: every 8192nd f32 magnitude (signs +0/+1), finite only.
    for (uint32_t bits = 1; bits < 0x7FFFFF; bits += 8192)
    {
        float v;
        std::memcpy(&v, &bits, 4);
        if (!std::isfinite(v)) continue;
        printf("f32\t%.9g\t%s\n", (double)v, fmt32(v).c_str());
        printf("f32\t%.9g\t%s\n", (double)-v, fmt32(-v).c_str());
    }
    return 0;
}
